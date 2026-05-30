"""Loom Runtime / Simulator — PySide6 GUI front-end for the `loom-play` binary.

Usage:
    pip install pyside6
    python simulator.py [project-dir]

Drives the Rust runtime over JSON-line stdio. The simulator is a thin client:
all narrative semantics live in `loom-runtime`. The Python side just
visualises and forwards mutation intents.

UI surfaces:
    - Transcript + choice buttons (top-left)
    - Ledger pane (raw event stream, top-right)
    - World tree (entity-grouped, inline-editable — edits round-trip as
      `<set: key = value>` directives so reactive hooks still fire)
    - Graph canvas (Obsidian-style force-directed layout for
      characters / locations / cohorts / beats; per-edge spring weights
      so story-spine and containment edges cluster tightly while social
      trust edges drift loosely; drag a node to pin it, double-click to
      open the inspector for live world editing)
    - Diagnostics pane

Stops cleanly on window close.
"""

from __future__ import annotations

import json
import math
import os
import random
import shutil
import sys
from pathlib import Path
from typing import Optional

from PySide6.QtCore import QPointF, QProcess, QRectF, Qt, QByteArray, QTimer, Signal
from PySide6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QPainter,
    QPainterPath,
    QPen,
    QTextCursor,
)
from PySide6.QtWidgets import (
    QApplication,
    QDialog,
    QDialogButtonBox,
    QDoubleSpinBox,
    QFileDialog,
    QFormLayout,
    QGraphicsItem,
    QGraphicsLineItem,
    QGraphicsPathItem,
    QGraphicsScene,
    QGraphicsSimpleTextItem,
    QGraphicsView,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMainWindow,
    QPlainTextEdit,
    QPushButton,
    QSplitter,
    QStatusBar,
    QTabWidget,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)


REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_BIN = REPO_ROOT / "target" / "debug" / "loom-play"
DEFAULT_PROJECT = REPO_ROOT / "packages" / "loom" / "examples" / "circuit-break"


def find_binary() -> Optional[Path]:
    debug = REPO_ROOT / "target" / "debug" / "loom-play"
    release = REPO_ROOT / "target" / "release" / "loom-play"
    profile = os.environ.get("LOOM_PLAY_PROFILE", "").lower()
    if profile == "release" and release.exists():
        return release
    on_path = shutil.which("loom-play")
    if on_path:
        return Path(on_path)
    if debug.exists():
        return debug
    if release.exists():
        return release
    return None


# ---------------------------------------------------------------------------
# Graph canvas
# ---------------------------------------------------------------------------

NODE_COLORS = {
    "character": QColor("#e0a800"),
    "cohort": QColor("#7c4dff"),
    "location": QColor("#26a69a"),
    "beat": QColor("#5c6bc0"),
    "item": QColor("#ef5350"),
}

EDGE_COLORS = {
    "divert": QColor("#90a4ae"),
    "choice": QColor("#80cbc4"),
    "setting": QColor("#26a69a"),
    "cast": QColor("#e0a800"),
    "trust": QColor("#ffb74d"),
    "contains": QColor("#4db6ac"),
    "current": QColor("#ff5252"),
}

# Per-edge spring parameters used by the force simulation. `rest` is the
# preferred edge length in pixels; `k` is Hooke's constant (higher = stiffer
# pull). Story-spine edges (diverts/choices) and physical containment are
# stiff and short so they form tight clusters; cast/setting are medium; trust
# is long and slack so social relationships can spread without dragging
# unrelated beats together. Multiple edges between the same pair *stack* —
# that's how "weightier" relationships emerge in Obsidian-style layouts.
EDGE_SPRINGS = {
    "divert":   {"rest": 105.0, "k": 0.045},
    "choice":   {"rest": 125.0, "k": 0.035},
    "contains": {"rest":  95.0, "k": 0.050},
    "setting":  {"rest": 150.0, "k": 0.022},
    "cast":     {"rest": 170.0, "k": 0.018},
    "trust":    {"rest": 230.0, "k": 0.007},
}
DEFAULT_SPRING = {"rest": 160.0, "k": 0.020}

# Node masses — heavier nodes resist getting flung around by springs/repulsion
# so they act as anchors. Beats and locations are the spatial/narrative
# skeleton; characters orbit; cohorts drift outward.
NODE_MASS = {
    "beat":      1.5,
    "location":  1.6,
    "character": 1.0,
    "cohort":    0.7,
    "item":      0.6,
}


class GraphNode(QGraphicsPathItem):
    """One node on the graph canvas. Shape encodes kind (circle for
    characters, square for locations, diamond for cohorts, rounded
    rect for beats). Stores a back-pointer to the entity record so
    the inspector can read/write it on double-click."""

    RADIUS = 26

    def __init__(self, key: str, kind: str, label: str, record: dict):
        super().__init__()
        self.key = key
        self.kind = kind
        self.label = label
        self.record = record
        self.setFlag(QGraphicsItem.ItemIsMovable, True)
        self.setFlag(QGraphicsItem.ItemIsSelectable, True)
        self.setFlag(QGraphicsItem.ItemSendsGeometryChanges, True)
        self.setAcceptHoverEvents(True)
        self.setZValue(2)

        path = QPainterPath()
        r = self.RADIUS
        if kind == "character":
            path.addEllipse(-r, -r, 2 * r, 2 * r)
        elif kind == "location":
            path.addRoundedRect(-r, -r, 2 * r, 2 * r, 4, 4)
        elif kind == "cohort":
            poly = [
                QPointF(0, -r),
                QPointF(r, 0),
                QPointF(0, r),
                QPointF(-r, 0),
            ]
            path.moveTo(poly[0])
            for p in poly[1:]:
                path.lineTo(p)
            path.closeSubpath()
        else:  # beat / item
            path.addRoundedRect(-r * 1.4, -r * 0.7, r * 2.8, r * 1.4, 8, 8)
        self.setPath(path)

        base = NODE_COLORS.get(kind, QColor("#888"))
        self.base_color = base
        self.setBrush(QBrush(base))
        self.setPen(QPen(QColor("#222"), 1.5))

        self.text = QGraphicsSimpleTextItem(label, self)
        font = QFont("Menlo", 8)
        self.text.setFont(font)
        rect = self.text.boundingRect()
        self.text.setPos(-rect.width() / 2, r + 2)
        self.text.setBrush(QBrush(QColor("#eceff1")))

        self.edges: list[GraphEdge] = []
        self._current = False
        # Force-sim state. Velocity carries momentum between ticks; `pinned`
        # freezes the node while the user is dragging it (and optionally
        # latched after release if we ever expose a pin gesture).
        self.vx = 0.0
        self.vy = 0.0
        self.pinned = False
        self.mass = NODE_MASS.get(kind, 1.0)

    def itemChange(self, change, value):
        if change == QGraphicsItem.ItemPositionHasChanged:
            for edge in self.edges:
                edge.adjust()
        return super().itemChange(change, value)

    def mousePressEvent(self, event):  # noqa: N802 — Qt API
        # Pin while the user is actively dragging so the simulation can't
        # fight the mouse. Released on mouseRelease (below) so the layout
        # relaxes around the new position.
        self.pinned = True
        self.vx = self.vy = 0.0
        super().mousePressEvent(event)

    def mouseReleaseEvent(self, event):  # noqa: N802 — Qt API
        self.pinned = False
        view = self.scene().views()[0] if self.scene().views() else None
        if view is not None and hasattr(view, "reheat"):
            view.reheat(0.6)
        super().mouseReleaseEvent(event)

    def mouseDoubleClickEvent(self, event):  # noqa: N802 — Qt API
        view = self.scene().views()[0] if self.scene().views() else None
        if view is not None and hasattr(view, "node_double_clicked"):
            view.node_double_clicked.emit(self)
        super().mouseDoubleClickEvent(event)

    def set_current(self, on: bool) -> None:
        self._current = on
        if on:
            self.setPen(QPen(QColor("#ff5252"), 3))
        else:
            self.setPen(QPen(QColor("#222"), 1.5))


class GraphEdge(QGraphicsLineItem):
    def __init__(self, src: GraphNode, dst: GraphNode, kind: str, label: str = ""):
        super().__init__()
        self.src = src
        self.dst = dst
        self.kind = kind
        spring = EDGE_SPRINGS.get(kind, DEFAULT_SPRING)
        self.rest_length = spring["rest"]
        self.spring_k = spring["k"]
        self.setZValue(1)
        color = EDGE_COLORS.get(kind, QColor("#777"))
        pen = QPen(color, 1.3)
        if kind in {"divert", "choice"}:
            pen.setStyle(Qt.SolidLine)
            pen.setWidthF(1.8)
        elif kind == "trust":
            pen.setStyle(Qt.DashLine)
        else:
            pen.setStyle(Qt.DotLine)
        self.setPen(pen)
        src.edges.append(self)
        dst.edges.append(self)
        self.adjust()

    def adjust(self) -> None:
        a, b = self.src.pos(), self.dst.pos()
        self.setLine(a.x(), a.y(), b.x(), b.y())


class GraphView(QGraphicsView):
    node_double_clicked = Signal(object)

    # Force-sim tuning. Repulsion is Coulomb-like (k_rep / r²) with a soft
    # near-field floor to avoid blow-up when nodes overlap. Gravity is a
    # weak linear pull toward the origin so disconnected components don't
    # drift off-screen.
    REPULSION_K = 6500.0
    REPULSION_CUTOFF = 480.0          # px — ignore far pairs (Barnes-Hut lite)
    REPULSION_MIN_DIST = 14.0         # px — clamp near-field 1/r²
    GRAVITY_K = 0.0065
    DAMPING = 0.78
    MAX_SPEED = 60.0                  # px / step — velocity cap
    ALPHA_DECAY = 0.992
    ALPHA_FLOOR = 0.005               # stop ticking below this

    def __init__(self):
        super().__init__()
        self._scene = QGraphicsScene(self)
        self._scene.setBackgroundBrush(QBrush(QColor("#1b1f24")))
        self.setScene(self._scene)
        self.setRenderHint(QPainter.Antialiasing)
        self.setDragMode(QGraphicsView.ScrollHandDrag)
        self.setTransformationAnchor(QGraphicsView.AnchorUnderMouse)
        self.nodes: dict[str, GraphNode] = {}
        self.edges: list[GraphEdge] = []
        self._current_node: Optional[GraphNode] = None

        # Force simulation. `alpha` is a heating coefficient that decays
        # toward zero so the layout settles, and gets re-kicked to 1.0
        # whenever the graph is repopulated or the user drags a node.
        self._alpha = 0.0
        self._sim_timer = QTimer(self)
        self._sim_timer.setInterval(16)  # ~60 fps
        self._sim_timer.timeout.connect(self._step_simulation)

    def reheat(self, alpha: float = 1.0) -> None:
        """Reset the simulation's heat coefficient and (re)start the timer."""
        self._alpha = max(self._alpha, min(1.0, alpha))
        if not self._sim_timer.isActive() and self._alpha > self.ALPHA_FLOOR:
            self._sim_timer.start()

    def wheelEvent(self, event):  # noqa: N802 — Qt API
        factor = 1.15 if event.angleDelta().y() > 0 else 1 / 1.15
        self.scale(factor, factor)

    def clear_graph(self) -> None:
        self._scene.clear()
        self.nodes.clear()
        self.edges.clear()
        self._current_node = None

    def add_node(self, key: str, kind: str, label: str, record: dict, pos: QPointF) -> GraphNode:
        node = GraphNode(key, kind, label, record)
        node.setPos(pos)
        self._scene.addItem(node)
        self.nodes[key] = node
        return node

    def add_edge(self, src_key: str, dst_key: str, kind: str) -> None:
        src = self.nodes.get(src_key)
        dst = self.nodes.get(dst_key)
        if src is None or dst is None or src is dst:
            return
        edge = GraphEdge(src, dst, kind)
        self._scene.addItem(edge)
        self.edges.append(edge)

    def populate(self, entities: dict) -> None:
        """Seed the canvas with one node per entity, then hand off to the
        force simulation (`_step_simulation`) for the actual layout.

        Initial positions are concentric rings (characters near the
        centre, locations next, cohorts outside, beats outermost) so the
        sim has a non-degenerate starting configuration; from there
        Coulomb repulsion + per-edge Hooke springs + weak gravity at
        the origin reorganise into Obsidian-style clusters. Positions
        survive across `entities` refreshes when a key is already on the
        scene, so hot-reload doesn't reshuffle the whole graph."""
        prev_positions = {key: node.pos() for key, node in self.nodes.items()}
        self.clear_graph()

        rings = [
            ("character", entities.get("characters", []), 0, "name"),
            ("location", entities.get("locations", []), 180, "name"),
            ("cohort", entities.get("cohorts", []), 320, "name"),
            ("beat", entities.get("beats", []), 480, "name"),
        ]
        for kind, records, radius, key_field in rings:
            if not records:
                continue
            count = max(len(records), 1)
            for i, rec in enumerate(records):
                name = rec.get(key_field) or rec.get("name") or f"<{kind} {i}>"
                key = f"{kind}:{name}"
                label = rec.get("label") or name
                if radius == 0:
                    angle = 2 * math.pi * i / count
                    pos = QPointF(60 * math.cos(angle), 60 * math.sin(angle))
                else:
                    angle = 2 * math.pi * i / count + (radius * 0.0001)
                    pos = QPointF(radius * math.cos(angle), radius * math.sin(angle))
                pos = prev_positions.get(key, pos)
                self.add_node(key, kind, label, rec, pos)

        # Edges --------------------------------------------------------------
        beat_names = {b["name"]: f"beat:{b['name']}" for b in entities.get("beats", [])}
        char_names = {c["name"]: f"character:{c['name']}" for c in entities.get("characters", [])}
        loc_names = {l["name"]: f"location:{l['name']}" for l in entities.get("locations", [])}
        cohort_names = {c["name"]: f"cohort:{c['name']}" for c in entities.get("cohorts", [])}

        # Divert + choice + cast + setting edges from beats.
        for beat in entities.get("beats", []):
            src = f"beat:{beat['name']}"
            for tgt in beat.get("diverts", []):
                if tgt in beat_names:
                    self.add_edge(src, beat_names[tgt], "divert")
            for choice in beat.get("choices", []):
                for tgt in choice.get("targets", []):
                    if tgt in beat_names:
                        self.add_edge(src, beat_names[tgt], "choice")
            cast = (beat.get("cast") or "").strip()
            if cast:
                for who in [c.strip() for c in cast.split(",") if c.strip()]:
                    if who in char_names:
                        self.add_edge(src, char_names[who], "cast")
            setting = (beat.get("setting") or "").strip()
            if setting and setting in loc_names:
                self.add_edge(src, loc_names[setting], "setting")

        # Character trust → cohort edges.
        for ch in entities.get("characters", []):
            src = f"character:{ch['name']}"
            for d in ch.get("disposition", []):
                target = d.get("target", "")
                if target in cohort_names:
                    self.add_edge(src, cohort_names[target], "trust")
                elif target in char_names:
                    self.add_edge(src, char_names[target], "trust")

        # Location contains.
        for loc in entities.get("locations", []):
            src = f"location:{loc['name']}"
            for child in loc.get("contains", []):
                if child in loc_names:
                    self.add_edge(src, loc_names[child], "contains")

        rect = self._scene.itemsBoundingRect().adjusted(-80, -80, 80, 80)
        self._scene.setSceneRect(rect)

        # Kick the force simulation. The ring layout above is just the
        # seed; the sim will reorganise into Obsidian-style clusters
        # weighted by edge kind and multiplicity.
        self.reheat(1.0)

    def _step_simulation(self) -> None:
        """One tick of the force-directed layout.

        Pairwise Coulomb repulsion + per-edge Hooke springs + a weak
        gravity well at the origin. We integrate with velocity Verlet-ish
        damping so the layout settles instead of oscillating, and the
        per-tick displacement is gated by `alpha` (a "heat" coefficient
        that decays toward zero — Obsidian's `alphaDecay` analogue).
        """
        if self._alpha <= self.ALPHA_FLOOR or not self.nodes:
            self._sim_timer.stop()
            return

        nodes = list(self.nodes.values())
        positions = [(n.pos().x(), n.pos().y()) for n in nodes]
        forces = [[0.0, 0.0] for _ in nodes]

        # Pairwise repulsion (O(N²) — fine for the <200-node graphs the
        # simulator deals with). Cutoff skips far pairs cheaply; the
        # near-field clamp keeps overlapping nodes from exploding.
        k_rep = self.REPULSION_K
        cutoff_sq = self.REPULSION_CUTOFF * self.REPULSION_CUTOFF
        min_d = self.REPULSION_MIN_DIST
        for i in range(len(nodes)):
            xi, yi = positions[i]
            fi = forces[i]
            for j in range(i + 1, len(nodes)):
                xj, yj = positions[j]
                dx = xj - xi
                dy = yj - yi
                d2 = dx * dx + dy * dy
                if d2 > cutoff_sq:
                    continue
                if d2 < min_d * min_d:
                    d2 = min_d * min_d
                d = math.sqrt(d2)
                f = k_rep / d2
                ux = dx / d
                uy = dy / d
                fi[0] -= f * ux
                fi[1] -= f * uy
                fj = forces[j]
                fj[0] += f * ux
                fj[1] += f * uy

        # Spring attraction along edges. Multiple edges between the same
        # pair stack — that's how "weightier" relationships (e.g. several
        # disposition lines to one cohort) pull tighter than a single
        # passing reference.
        index = {id(n): i for i, n in enumerate(nodes)}
        for edge in self.edges:
            ia = index.get(id(edge.src))
            ib = index.get(id(edge.dst))
            if ia is None or ib is None:
                continue
            xa, ya = positions[ia]
            xb, yb = positions[ib]
            dx = xb - xa
            dy = yb - ya
            d = math.sqrt(dx * dx + dy * dy)
            if d < 0.001:
                continue
            f = edge.spring_k * (d - edge.rest_length)
            ux = dx / d
            uy = dy / d
            fa = forces[ia]
            fb = forces[ib]
            fa[0] += f * ux
            fa[1] += f * uy
            fb[0] -= f * ux
            fb[1] -= f * uy

        # Weak gravity well at the origin keeps disconnected components
        # from drifting and centres the whole graph in the viewport.
        kg = self.GRAVITY_K
        for i, (x, y) in enumerate(positions):
            forces[i][0] -= kg * x
            forces[i][1] -= kg * y

        # Integrate with damping. Velocity-capped so a single spike can't
        # fling a node off-screen; alpha gates total motion so the system
        # cools instead of oscillating forever.
        damping = self.DAMPING
        alpha = self._alpha
        max_speed = self.MAX_SPEED
        for i, n in enumerate(nodes):
            if n.pinned:
                n.vx = n.vy = 0.0
                continue
            fx, fy = forces[i]
            n.vx = (n.vx + fx / n.mass) * damping
            n.vy = (n.vy + fy / n.mass) * damping
            speed = math.hypot(n.vx, n.vy)
            if speed > max_speed:
                scale = max_speed / speed
                n.vx *= scale
                n.vy *= scale
            x, y = positions[i]
            n.setPos(x + n.vx * alpha, y + n.vy * alpha)

        self._alpha *= self.ALPHA_DECAY
        # Keep the scene rect generous so panning still works as the
        # cluster moves; refresh only every few frames to avoid churn.
        if self._scene.sceneRect().isEmpty() or random.random() < 0.05:
            rect = self._scene.itemsBoundingRect().adjusted(-200, -200, 200, 200)
            self._scene.setSceneRect(rect)

    def highlight_beat(self, beat_name: str) -> None:
        if self._current_node is not None:
            self._current_node.set_current(False)
        key = f"beat:{beat_name}"
        node = self.nodes.get(key)
        if node is not None:
            node.set_current(True)
            self._current_node = node
            self.centerOn(node)


# ---------------------------------------------------------------------------
# Inspector dialog (per-node)
# ---------------------------------------------------------------------------


class InspectorDialog(QDialog):
    """Edit live world values for one entity. Each editable field flushes
    through the `set` command, so the runtime applies the same hook /
    reaction / KnowledgeChanged bookkeeping it would for a scripted
    `<set:>` directive."""

    def __init__(self, parent, node: GraphNode, world: dict[str, str], on_set):
        super().__init__(parent)
        self.setWindowTitle(f"{node.kind.title()}: {node.label}")
        self.resize(420, 320)
        self.on_set = on_set
        layout = QVBoxLayout(self)

        header = QLabel(f"<b>{node.label}</b> &mdash; <i>{node.kind}</i>")
        header.setTextFormat(Qt.RichText)
        layout.addWidget(header)

        form = QFormLayout()
        layout.addLayout(form)

        editable_keys: list[tuple[str, str]] = []
        rec = node.record

        if node.kind == "character":
            for d in rec.get("disposition", []):
                key = f"{rec['name']}.{d['verb']}.{d['target']}"
                editable_keys.append((key, "number"))
            for k in rec.get("knowledge", []):
                key = f"{rec['name']}.knows.{k['field']}"
                schema = (k.get("schema") or "").strip()
                kind = "bool" if schema == "bool" else ("string" if "|" in schema else "string")
                editable_keys.append((key, kind))
        elif node.kind == "cohort":
            editable_keys.append((f"Cohort.{rec['name']}.size", "number"))
        elif node.kind == "location":
            editable_keys.append((f"Location.{rec['name']}.present", "number"))

        if not editable_keys:
            form.addRow(QLabel("(no editable runtime values)"))

        for key, kind in editable_keys:
            current = world.get(key, "")
            if kind == "number":
                widget = QDoubleSpinBox()
                widget.setRange(-1e9, 1e9)
                widget.setDecimals(2)
                try:
                    widget.setValue(float(current) if current else 0.0)
                except ValueError:
                    widget.setValue(0.0)
                widget.editingFinished.connect(
                    lambda k=key, w=widget: self._flush(k, w.value())
                )
            else:
                widget = QLineEdit(current)
                widget.editingFinished.connect(
                    lambda k=key, w=widget: self._flush(k, w.text())
                )
            form.addRow(key, widget)

        # All current world entries for this entity (read-only context).
        prefix = rec.get("name") or ""
        ctx = QPlainTextEdit()
        ctx.setReadOnly(True)
        mono = QFont("Menlo")
        ctx.setFont(mono)
        ctx_lines = [f"{k} = {v}" for k, v in world.items() if k.startswith(prefix + ".") or k == prefix]
        ctx.setPlainText("\n".join(ctx_lines) or "(no live world entries)")
        layout.addWidget(QLabel("Live state:"))
        layout.addWidget(ctx, 1)

        bb = QDialogButtonBox(QDialogButtonBox.Close)
        bb.rejected.connect(self.accept)
        bb.accepted.connect(self.accept)
        layout.addWidget(bb)

    def _flush(self, key: str, value) -> None:
        # Cast bool-shaped strings.
        if isinstance(value, str):
            lower = value.strip().lower()
            if lower in {"true", "false"}:
                value = lower == "true"
        self.on_set(key, value)


# ---------------------------------------------------------------------------
# Main window
# ---------------------------------------------------------------------------


class Simulator(QMainWindow):
    def __init__(self, project: Optional[Path] = None) -> None:
        super().__init__()
        self.setWindowTitle("Loom Simulator")
        self.resize(1480, 900)

        self.proc: Optional[QProcess] = None
        self.project_dir: Optional[Path] = project
        self._buffer = bytearray()
        self._choice_buttons: list[QPushButton] = []
        self._world_cache: dict[str, str] = {}
        self._entities_cache: dict = {}
        self._suppress_world_signal = False

        self._build_ui()
        self._wire_actions()

        if project is not None:
            self._start_runtime(project)

    # --- UI construction --------------------------------------------------

    def _build_ui(self) -> None:
        central = QWidget()
        self.setCentralWidget(central)
        outer = QVBoxLayout(central)

        # Top bar.
        topbar = QHBoxLayout()
        self.path_field = QLineEdit()
        self.path_field.setPlaceholderText("Project directory…")
        if self.project_dir:
            self.path_field.setText(str(self.project_dir))
        self.open_btn = QPushButton("Open…")
        self.start_btn = QPushButton("Start")
        self.restart_btn = QPushButton("Restart")
        self.reload_btn = QPushButton("Reload")
        self.skip_btn = QPushButton("Skip Beat")
        self.force_field = QLineEdit()
        self.force_field.setPlaceholderText("force directive (e.g. sfx: thunder)")
        self.force_btn = QPushButton("Fire")
        topbar.addWidget(QLabel("Project:"))
        topbar.addWidget(self.path_field, 1)
        topbar.addWidget(self.open_btn)
        topbar.addWidget(self.start_btn)
        topbar.addWidget(self.restart_btn)
        topbar.addWidget(self.reload_btn)
        topbar.addWidget(self.skip_btn)
        topbar.addWidget(self.force_field, 1)
        topbar.addWidget(self.force_btn)
        outer.addLayout(topbar)

        # Main split.
        split = QSplitter(Qt.Horizontal)
        outer.addWidget(split, 1)

        # --- Left: transcript + choices ---
        left = QWidget()
        lv = QVBoxLayout(left)
        lv.setContentsMargins(0, 0, 0, 0)
        self.transcript = QPlainTextEdit()
        self.transcript.setReadOnly(True)
        mono = QFont("Menlo")
        mono.setStyleHint(QFont.TypeWriter)
        self.transcript.setFont(mono)
        lv.addWidget(self.transcript, 1)

        self.choice_panel = QWidget()
        self.choice_layout = QVBoxLayout(self.choice_panel)
        self.choice_layout.setContentsMargins(8, 4, 8, 8)
        lv.addWidget(self.choice_panel)
        split.addWidget(left)

        # --- Right: tabs ---
        right = QTabWidget()
        self.ledger_view = QPlainTextEdit()
        self.ledger_view.setReadOnly(True)
        self.ledger_view.setFont(mono)
        right.addTab(self.ledger_view, "Ledger")

        # World tree (grouped, inline-editable).
        self.world_tree = QTreeWidget()
        self.world_tree.setColumnCount(2)
        self.world_tree.setHeaderLabels(["Entity / Key", "Value"])
        self.world_tree.setAlternatingRowColors(True)
        self.world_tree.itemChanged.connect(self._on_world_item_changed)
        right.addTab(self.world_tree, "World")

        # Graph canvas.
        self.graph = GraphView()
        self.graph.node_double_clicked.connect(self._open_inspector)
        right.addTab(self.graph, "Graph")

        self.diag_view = QPlainTextEdit()
        self.diag_view.setReadOnly(True)
        self.diag_view.setFont(mono)
        right.addTab(self.diag_view, "Diagnostics")

        split.addWidget(right)
        split.setSizes([800, 680])

        self.setStatusBar(QStatusBar())

    def _wire_actions(self) -> None:
        self.open_btn.clicked.connect(self._pick_project)
        self.start_btn.clicked.connect(self._start_from_field)
        self.restart_btn.clicked.connect(self._restart)
        self.reload_btn.clicked.connect(lambda: self._send({"cmd": "reload"}))
        self.skip_btn.clicked.connect(lambda: self._send({"cmd": "skip"}))
        self.force_btn.clicked.connect(self._fire_force)

    # --- Process management -----------------------------------------------

    def _pick_project(self) -> None:
        start = str(self.project_dir or DEFAULT_PROJECT.parent)
        chosen = QFileDialog.getExistingDirectory(self, "Choose Loom project", start)
        if chosen:
            self.path_field.setText(chosen)
            self._start_runtime(Path(chosen))

    def _start_from_field(self) -> None:
        text = self.path_field.text().strip()
        if text:
            self._start_runtime(Path(text))

    def _restart(self) -> None:
        if self.project_dir is not None:
            self._start_runtime(self.project_dir)

    def _start_runtime(self, project: Path) -> None:
        binary = find_binary()
        if binary is None:
            self.statusBar().showMessage(
                "loom-play binary not found — run `cargo build -p loom-runtime --bin loom-play`"
            )
            return

        if self.proc is not None:
            try:
                self._send({"cmd": "quit"})
            except Exception:
                pass
            self.proc.kill()
            self.proc.waitForFinished(500)

        self._reset_views()
        self.project_dir = project
        self.proc = QProcess(self)
        self.proc.setProgram(str(binary))
        self.proc.setArguments([str(project)])
        self.proc.readyReadStandardOutput.connect(self._on_stdout)
        self.proc.readyReadStandardError.connect(self._on_stderr)
        self.proc.finished.connect(self._on_finished)
        self.proc.start()
        self.statusBar().showMessage(f"Loaded {project}")
        # Ask for entities + world immediately after `ready`; we drive it
        # from _handle("ready") below.

    def closeEvent(self, event) -> None:  # noqa: N802 — Qt API
        if self.proc is not None and self.proc.state() == QProcess.Running:
            try:
                self._send({"cmd": "quit"})
            except Exception:
                pass
            self.proc.waitForFinished(500)
            if self.proc.state() == QProcess.Running:
                self.proc.kill()
        super().closeEvent(event)

    # --- Stdio plumbing ---------------------------------------------------

    def _send(self, payload: dict) -> None:
        if self.proc is None or self.proc.state() != QProcess.Running:
            return
        line = (json.dumps(payload) + "\n").encode("utf-8")
        self.proc.write(QByteArray(line))

    def _on_stdout(self) -> None:
        assert self.proc is not None
        data = bytes(self.proc.readAllStandardOutput())
        self._buffer.extend(data)
        while b"\n" in self._buffer:
            line, _, rest = self._buffer.partition(b"\n")
            self._buffer = bytearray(rest)
            text = line.decode("utf-8", errors="replace").strip()
            if not text:
                continue
            try:
                msg = json.loads(text)
            except json.JSONDecodeError:
                self._append_ledger(f"[non-json] {text}")
                continue
            self._handle(msg)

    def _on_stderr(self) -> None:
        assert self.proc is not None
        data = bytes(self.proc.readAllStandardError()).decode("utf-8", errors="replace")
        if data:
            self._append_ledger(f"[stderr] {data.rstrip()}")

    def _on_finished(self, *_: object) -> None:
        self.statusBar().showMessage("loom-play exited")

    # --- Message dispatch -------------------------------------------------

    def _handle(self, msg: dict) -> None:
        kind = msg.get("type")
        if kind == "ready":
            entry = msg.get("entry") or "<none>"
            self.statusBar().showMessage(f"Ready — entry: {entry}")
            self._set_diagnostics(msg.get("diagnostics") or [])
            self._send({"cmd": "entities"})
            self._send({"cmd": "world"})
        elif kind == "event":
            self._handle_event(msg.get("event") or {})
        elif kind == "choice":
            self._show_choices(msg.get("options") or [])
        elif kind == "awaiting":
            self._append_ledger(f"[awaiting coroutine {msg.get('coroutine')}]")
            self._send({"cmd": "step"})
        elif kind == "ended":
            self._append_transcript("\n— END —\n")
            self._clear_choices()
            self._send({"cmd": "world"})
        elif kind == "world":
            self._set_world(msg.get("entries") or [])
        elif kind == "entities":
            self._set_entities(msg)
        elif kind == "error":
            self._append_ledger(f"[error] {msg.get('message')}")
            self.statusBar().showMessage(msg.get("message") or "error")
        else:
            self._append_ledger(f"[?] {msg}")

    def _handle_event(self, event: dict) -> None:
        if not event:
            return
        ((kind, payload),) = event.items() if isinstance(event, dict) else (("?", {}),)
        self._append_ledger(f"{kind}: {self._summarize(payload)}")
        if kind == "Scene":
            self._append_transcript(f"\n[SCENE] {payload.get('text','')}\n")
        elif kind == "Action":
            self._append_transcript(f"\n{payload.get('text','')}\n")
        elif kind == "Dialogue":
            speakers = " | ".join(payload.get("speakers") or [payload.get("speaker", "?")])
            paren = payload.get("parenthetical")
            tag = f" ({paren})" if paren else ""
            self._append_transcript(f"\n  {speakers}{tag}\n    {payload.get('text','')}\n")
        elif kind == "BeatEntered":
            beat = payload.get("beat", "")
            self._append_transcript(f"\n══ {beat} ══\n")
            self.graph.highlight_beat(beat)
        elif kind == "Metadata":
            self._append_transcript(f"\n  ⟨{payload.get('text','')}⟩\n")
        elif kind in {"Diverted", "Tunneled"}:
            self._append_transcript(f"\n→ {payload.get('beat','')}\n")
        elif kind == "ChoiceTaken":
            self._append_transcript(f"\n>>> {payload.get('text','')}\n")
        if kind in {"WorldSet", "KnowledgeChanged", "LetEvaluated"}:
            self._send({"cmd": "world"})

    @staticmethod
    def _summarize(payload: object) -> str:
        if isinstance(payload, dict):
            bits = []
            for k, v in payload.items():
                s = str(v)
                if len(s) > 60:
                    s = s[:57] + "…"
                bits.append(f"{k}={s}")
            return ", ".join(bits)
        return str(payload)

    # --- View mutators ----------------------------------------------------

    def _reset_views(self) -> None:
        self.transcript.clear()
        self.ledger_view.clear()
        self.diag_view.clear()
        self.world_tree.clear()
        self.graph.clear_graph()
        self._world_cache.clear()
        self._entities_cache = {}
        self._clear_choices()

    def _append_transcript(self, text: str) -> None:
        self.transcript.moveCursor(QTextCursor.End)
        self.transcript.insertPlainText(text)
        self.transcript.moveCursor(QTextCursor.End)

    def _append_ledger(self, line: str) -> None:
        self.ledger_view.appendPlainText(line)

    def _set_diagnostics(self, items: list) -> None:
        self.diag_view.clear()
        if not items:
            self.diag_view.appendPlainText("(none)")
            return
        for d in items:
            kind = d.get("kind", "?")
            msg = d.get("message", "")
            file = d.get("file")
            prefix = f"[{kind}]" + (f" {file}" if file else "")
            self.diag_view.appendPlainText(f"{prefix}  {msg}")

    def _set_world(self, entries: list) -> None:
        """Re-render the World tree from a flat `[(key, display)]` snapshot.
        Keys are dotted (`Vex.trusts.Chatters`); we group by the leading
        segment. Tree state is rebuilt rather than diffed — the snapshot
        is small enough that the simplicity is worth it."""
        self._suppress_world_signal = True
        self.world_tree.clear()
        self._world_cache = {k: v for k, v in entries if isinstance(k, str)}

        groups: dict[str, QTreeWidgetItem] = {}

        def get_group(name: str) -> QTreeWidgetItem:
            if name not in groups:
                top = QTreeWidgetItem([name, ""])
                top.setFirstColumnSpanned(False)
                top.setFlags(top.flags() & ~Qt.ItemIsEditable)
                font = top.font(0)
                font.setBold(True)
                top.setFont(0, font)
                self.world_tree.addTopLevelItem(top)
                top.setExpanded(True)
                groups[name] = top
            return groups[name]

        for key, value in entries:
            parts = key.split(".")
            if len(parts) == 1:
                # Top-level under a synthetic "(globals)" group.
                grp = get_group("(globals)")
                node = QTreeWidgetItem([key, value])
                node.setFlags(node.flags() | Qt.ItemIsEditable)
                node.setData(0, Qt.UserRole, key)
                grp.addChild(node)
            else:
                grp = get_group(parts[0])
                # Drill down through middle segments.
                parent = grp
                for mid in parts[1:-1]:
                    found = None
                    for i in range(parent.childCount()):
                        ch = parent.child(i)
                        if ch.text(0) == mid and ch.data(0, Qt.UserRole) is None:
                            found = ch
                            break
                    if found is None:
                        found = QTreeWidgetItem([mid, ""])
                        found.setFlags(found.flags() & ~Qt.ItemIsEditable)
                        parent.addChild(found)
                    parent = found
                    parent.setExpanded(True)
                leaf = QTreeWidgetItem([parts[-1], value])
                leaf.setFlags(leaf.flags() | Qt.ItemIsEditable)
                leaf.setData(0, Qt.UserRole, key)
                parent.addChild(leaf)

        self.world_tree.resizeColumnToContents(0)
        self._suppress_world_signal = False

    def _on_world_item_changed(self, item: QTreeWidgetItem, column: int) -> None:
        if self._suppress_world_signal or column != 1:
            return
        key = item.data(0, Qt.UserRole)
        if not key:
            return
        new_value = item.text(1)
        if self._world_cache.get(key) == new_value:
            return
        # Coerce to the most natural literal.
        coerced: object = new_value
        try:
            if new_value.lower() in {"true", "false"}:
                coerced = new_value.lower() == "true"
            else:
                coerced = float(new_value)
                if coerced.is_integer():
                    coerced = int(coerced)
        except (ValueError, AttributeError):
            coerced = new_value
        self._send({"cmd": "set", "key": key, "value": coerced})

    def _set_entities(self, msg: dict) -> None:
        self._entities_cache = msg
        self.graph.populate(msg)

    def _open_inspector(self, node: GraphNode) -> None:
        dlg = InspectorDialog(self, node, self._world_cache, self._set_world_value)
        dlg.exec()

    def _set_world_value(self, key: str, value) -> None:
        self._send({"cmd": "set", "key": key, "value": value})

    def _clear_choices(self) -> None:
        for btn in self._choice_buttons:
            btn.deleteLater()
        self._choice_buttons.clear()

    def _show_choices(self, options: list) -> None:
        self._clear_choices()
        for opt in options:
            idx = opt.get("index", 0)
            text = opt.get("text", f"<choice {idx}>")
            sticky = " ◆" if opt.get("sticky") else ""
            btn = QPushButton(f"{idx + 1}. {text}{sticky}")
            btn.clicked.connect(lambda _=False, i=idx: self._take_choice(i))
            self.choice_layout.addWidget(btn)
            self._choice_buttons.append(btn)

    def _take_choice(self, index: int) -> None:
        self._clear_choices()
        self._send({"cmd": "choose", "index": index})

    def _fire_force(self) -> None:
        raw = self.force_field.text().strip()
        if not raw:
            return
        self._send({"cmd": "force", "raw": raw})
        self.force_field.clear()


def main() -> int:
    project = None
    if len(sys.argv) > 1:
        project = Path(sys.argv[1]).resolve()
    elif DEFAULT_PROJECT.exists():
        project = DEFAULT_PROJECT

    app = QApplication(sys.argv)
    win = Simulator(project)
    win.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
