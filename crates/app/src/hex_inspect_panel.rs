//! Sprint 3.5.E DD7: read-only panel showing the attributes of the
//! currently-picked hex (from Runtime.picked_hex).
//!
//! Composes with egui_dock as TabKind::HexInspect. Empty-state when no
//! hex is picked; "pipeline not populated" state when derived caches
//! haven't been written yet.

use hex::OffsetCoord;
use island_core::world::WorldState;

const DASH: &str = "—";

pub struct HexInspectPanel;

impl HexInspectPanel {
    pub fn show(ui: &mut egui::Ui, world: &WorldState, picked_hex: Option<OffsetCoord>) {
        match picked_hex {
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label("No hex selected — click in the viewport");
                });
            }
            Some(picked) => {
                Self::show_attrs(ui, world, picked);
            }
        }
    }

    fn show_attrs(ui: &mut egui::Ui, world: &WorldState, picked: OffsetCoord) {
        let (hex_attrs, hex_coast_class) = match (
            world.derived.hex_attrs.as_ref(),
            world.derived.hex_coast_class.as_ref(),
        ) {
            (Some(a), Some(c)) => (a, c),
            _ => {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label("Pipeline not yet populated");
                });
                return;
            }
        };

        let col = picked.col;
        let row = picked.row;

        // Bounds check (OffsetCoord is u32; picked should be in-range from pixel_to_offset).
        if col >= hex_attrs.cols || row >= hex_attrs.rows {
            ui.label(format!("Out-of-range hex ({col}, {row})"));
            return;
        }

        let hex_id = (row * hex_attrs.cols + col) as usize;
        let attrs = hex_attrs.get(col, row);

        // Axial conversion (odd-r offset → axial).
        let q = col as i32 - (row as i32 - (row as i32 & 1)) / 2;
        let r = row as i32;

        egui::Grid::new("hex_inspect_grid")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("offset (col, row)");
                ui.label(format!("({col}, {row})"));
                ui.end_row();

                ui.label("axial (q, r)");
                ui.label(format!("({q}, {r})"));
                ui.end_row();

                ui.label("elevation");
                ui.label(format!("{:.3}", attrs.elevation));
                ui.end_row();

                ui.label("dominant biome");
                ui.label(format!("{:?}", attrs.dominant_biome));
                ui.end_row();

                ui.label("has_river");
                ui.label(if attrs.has_river { "yes" } else { "no" });
                ui.end_row();

                ui.label("moisture");
                ui.label(format!("{:.3}", attrs.moisture));
                ui.end_row();

                ui.label("temperature");
                ui.label(format!("{:.3}", attrs.temperature));
                ui.end_row();

                ui.label("slope");
                ui.label(format!("{:.3}", attrs.slope));
                ui.end_row();

                ui.label("rainfall");
                ui.label(format!("{:.3}", attrs.rainfall));
                ui.end_row();

                ui.label("coast class");
                ui.label(
                    hex_coast_class
                        .get(hex_id)
                        .map(|cc| format!("{cc:?}"))
                        .unwrap_or_else(|| DASH.to_string()),
                );
                ui.end_row();

                ui.label("river crossing");
                let crossing_text = world
                    .derived
                    .hex_debug
                    .as_ref()
                    .and_then(|dbg| {
                        let cross = dbg.river_crossing.get(hex_id)?.as_ref()?;
                        let width = dbg.river_width.get(hex_id)?.as_ref();
                        let entry = hex::geometry::HexEdge::from_u8(cross.entry_edge)?;
                        let exit = hex::geometry::HexEdge::from_u8(cross.exit_edge)?;
                        Some(match width {
                            Some(w) => format!("{entry:?} → {exit:?} ({w:?})"),
                            None => format!("{entry:?} → {exit:?}"),
                        })
                    })
                    .unwrap_or_else(|| DASH.to_string());
                ui.label(crossing_text);
                ui.end_row();

                ui.label("accessibility cost");
                let cost_text = world
                    .derived
                    .hex_debug
                    .as_ref()
                    .and_then(|dbg| dbg.accessibility_cost.get(hex_id).copied())
                    .map(|c| format!("{c:.3}"))
                    .unwrap_or_else(|| DASH.to_string());
                ui.label(cost_text);
                ui.end_row();
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::{seed::Seed, world::Resolution};

    fn make_world() -> WorldState {
        let preset = data::presets::load_preset("volcanic_single")
            .expect("volcanic_single preset must load for tests");
        WorldState::new(Seed(0), preset, Resolution::new(32, 32))
    }

    fn run_panel(world: &WorldState, picked: Option<OffsetCoord>) {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());
        #[allow(deprecated)]
        egui::CentralPanel::default().show(&ctx, |ui| {
            HexInspectPanel::show(ui, world, picked);
        });
        let _ = ctx.end_pass();
    }

    /// Smoke-test: the empty-state path (no picked hex) must not panic.
    #[test]
    fn empty_state_no_panic() {
        let world = make_world();
        run_panel(&world, None);
    }

    /// Smoke-test: the "pipeline not populated" path must not panic.
    ///
    /// A freshly-constructed `WorldState` has `derived.hex_attrs = None`
    /// so the panel should bail with the "Pipeline not yet populated" label.
    #[test]
    fn pipeline_not_populated_no_panic() {
        let world = make_world();
        // derived.hex_attrs is None on a freshly-constructed WorldState.
        run_panel(&world, Some(OffsetCoord { col: 0, row: 0 }));
    }
}
