/**
 * CalendarWidgetRenderer — month/week/day calendar of kernel objects.
 *
 * Reads objects matching `collectionType`, plots them onto calendar cells
 * by `dateField`. Pure CSS grid — no calendar library.
 */

import { useMemo, useState } from "react";
import type { GraphObject } from "@prism/core/object-model";

export interface CalendarWidgetProps {
  objects: GraphObject[];
  dateField: string;
  titleField: string;
  viewType?: "month" | "week" | "day";
  onSelectObject?: (id: string) => void;
  onCreateAtDate?: (isoDate: string) => void;
}

interface DayCell {
  isoDate: string;
  dayOfMonth: number;
  inMonth: boolean;
  events: GraphObject[];
}

/** Build a 6x7 month grid (Sun..Sat) containing the anchor date. */
export function buildMonthGrid(
  anchor: Date,
  events: GraphObject[],
  dateField: string,
): DayCell[] {
  const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
  const gridStart = new Date(first);
  gridStart.setDate(1 - first.getDay()); // back to Sunday
  const cells: DayCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(gridStart);
    d.setDate(gridStart.getDate() + i);
    const iso = toIsoDate(d);
    cells.push({
      isoDate: iso,
      dayOfMonth: d.getDate(),
      inMonth: d.getMonth() === anchor.getMonth(),
      events: events.filter((o) => readDateField(o, dateField) === iso),
    });
  }
  return cells;
}

export function toIsoDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${dd}`;
}

export function readDateField(obj: GraphObject, field: string): string | null {
  const data = obj.data as Record<string, unknown>;
  const raw = data[field];
  if (typeof raw !== "string") return null;
  // Accept full ISO timestamps too — strip the time part.
  const match = raw.match(/^(\d{4}-\d{2}-\d{2})/);
  return match ? match[1] ?? null : null;
}

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;

export function CalendarWidgetRenderer(props: CalendarWidgetProps) {
  const { objects, dateField, titleField, onSelectObject, onCreateAtDate } = props;
  const [anchor, setAnchor] = useState(() => new Date());

  const cells = useMemo(
    () => buildMonthGrid(anchor, objects, dateField),
    [anchor, objects, dateField],
  );

  const goPrev = (): void => setAnchor(new Date(anchor.getFullYear(), anchor.getMonth() - 1, 1));
  const goNext = (): void => setAnchor(new Date(anchor.getFullYear(), anchor.getMonth() + 1, 1));
  const goToday = (): void => setAnchor(new Date());

  return (
    <div
      data-testid="calendar-widget"
      style={{
        border: "1px solid #22c55e",
        borderRadius: 6,
        background: "#0f172a",
        padding: 8,
        color: "#e2e8f0",
        fontSize: 12,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
        <div style={{ display: "flex", gap: 4 }}>
          <button type="button" onClick={goPrev} style={btnStyle} aria-label="Previous month">
            ‹
          </button>
          <button type="button" onClick={goToday} style={btnStyle}>
            Today
          </button>
          <button type="button" onClick={goNext} style={btnStyle} aria-label="Next month">
            ›
          </button>
        </div>
        <div style={{ fontWeight: 600 }}>
          {MONTHS[anchor.getMonth()]} {anchor.getFullYear()}
        </div>
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(7, 1fr)",
          gap: 1,
          background: "#1e293b",
          border: "1px solid #1e293b",
        }}
      >
        {WEEKDAYS.map((w) => (
          <div
            key={w}
            style={{
              padding: "4px 6px",
              background: "#1e293b",
              color: "#94a3b8",
              fontWeight: 600,
              textAlign: "center",
            }}
          >
            {w}
          </div>
        ))}

        {cells.map((cell) => (
          <div
            key={cell.isoDate}
            data-testid={`calendar-cell-${cell.isoDate}`}
            onClick={(e) => {
              if (e.target === e.currentTarget && onCreateAtDate) onCreateAtDate(cell.isoDate);
            }}
            style={{
              minHeight: 60,
              padding: 4,
              background: cell.inMonth ? "#0f172a" : "#111827",
              color: cell.inMonth ? "#e2e8f0" : "#475569",
              cursor: onCreateAtDate ? "pointer" : "default",
            }}
          >
            <div style={{ fontSize: 10, opacity: 0.7 }}>{cell.dayOfMonth}</div>
            {cell.events.slice(0, 3).map((ev) => {
              const title = String((ev.data as Record<string, unknown>)[titleField] ?? ev.id);
              return (
                <div
                  key={ev.id}
                  data-testid={`calendar-event-${ev.id}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (onSelectObject) onSelectObject(ev.id);
                  }}
                  style={{
                    fontSize: 10,
                    background: "#22c55e",
                    color: "#0f172a",
                    borderRadius: 3,
                    padding: "1px 4px",
                    marginTop: 2,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    cursor: "pointer",
                  }}
                  title={title}
                >
                  {title}
                </div>
              );
            })}
            {cell.events.length > 3 ? (
              <div style={{ fontSize: 10, color: "#94a3b8", marginTop: 2 }}>
                +{cell.events.length - 3} more
              </div>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}

const btnStyle: React.CSSProperties = {
  background: "#1e293b",
  border: "1px solid #334155",
  color: "#e2e8f0",
  padding: "2px 8px",
  borderRadius: 4,
  cursor: "pointer",
  fontSize: 12,
};
