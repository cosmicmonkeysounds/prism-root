//! Go-to-definition for diverts (-> beat) and CHARACTER references
//! inside dialogue cues.

use lsp_types::{GotoDefinitionResponse, Location, Position, Url};

use crate::hover::token_at;
use crate::workspace::{line_at, Workspace};

pub fn definition_at(ws: &Workspace, uri: &Url, pos: Position) -> Option<GotoDefinitionResponse> {
    let doc = ws.docs.get(uri)?;
    let line = line_at(&doc.text, pos.line);
    let (token, start, _) = token_at(line, pos.character as usize)?;

    // Divert: `-> token` form.
    if let Some(arrow) = line[..start].rfind("->") {
        if line[arrow + 2..start].trim().is_empty() {
            if let Some(entries) = ws.beats.get(token) {
                let locs: Vec<Location> = entries
                    .iter()
                    .map(|b| Location {
                        uri: b.uri.clone(),
                        range: b.name_range,
                    })
                    .collect();
                if !locs.is_empty() {
                    return Some(GotoDefinitionResponse::Array(locs));
                }
            }
        }
    }

    // CHARACTER reference: ALL-CAPS token.
    if token.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
        if let Some(info) = ws.characters.get(token) {
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: info.uri.clone(),
                range: info.range,
            }));
        }
    }

    None
}
