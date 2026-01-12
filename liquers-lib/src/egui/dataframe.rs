use egui::{Color32, RichText};
use polars::prelude::*;
use egui_extras::{TableBuilder, Column};

/// Display a polars DataFrame as a sortable table in egui.
/// Data types are indicated by color:
/// - Positive numbers: green
/// - Negative numbers: red
/// - Strings: default color
pub fn display_polars_dataframe(ui: &mut egui::Ui, df: &DataFrame, sort_state: &mut Option<(usize, bool)>) -> egui::Response {
    let columns = df.get_columns();
    let nrows = df.height();
    let ncols = columns.len();
    let col_names: Vec<_> = df.get_column_names().iter().map(|s| s.to_string()).collect();

    // Sorting logic
    if let Some((col_idx, ascending)) = *sort_state {
        let col_name = &col_names[col_idx];
        let by: Vec<String> = vec![col_name.clone()];
        let options = polars::prelude::SortMultipleOptions {
            descending: vec![!ascending],
            ..Default::default()
        };
        if let Ok(sorted_df) = df.sort(&by, options) {
            return display_polars_dataframe(ui, &sorted_df, sort_state);
        }
    }

    ui.vertical(|ui| {
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto(), ncols)
            .header(20.0, |mut header| {
                for (i, name) in col_names.iter().enumerate() {
                    header.col(|ui| {
                        let label = if let Some((sort_idx, asc)) = *sort_state {
                            if sort_idx == i {
                                if asc {
                                    format!("{} ↑", name)
                                } else {
                                    format!("{} ↓", name)
                                }
                            } else {
                                name.clone()
                            }
                        } else {
                            name.clone()
                        };
                        if ui.button(label).clicked() {
                            match *sort_state {
                                Some((idx, asc)) if idx == i => *sort_state = Some((i, !asc)),
                                _ => *sort_state = Some((i, true)),
                            }
                        }
                    });
                }
            })
            .body(|mut body| {
                for row_idx in 0..nrows {
                    body.row(18.0, |mut row| {
                        for col in columns {
                            row.col(|ui| {
                                let dtype = col.dtype();
                                match dtype {
                                    DataType::Int64 => {
                                        let v = col.i64().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0 { Color32::GREEN } else { Color32::RED };
                                            ui.label(RichText::new(val.to_string()).color(color));
                                        } else {
                                            ui.label("-");
                                        }
                                    }
                                    DataType::Int32 => {
                                        let v = col.i32().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0 { Color32::GREEN } else { Color32::RED };
                                            ui.label(RichText::new(val.to_string()).color(color));
                                        } else {
                                            ui.label("-");
                                        }
                                    }
                                    DataType::Float64 => {
                                        let v = col.f64().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0.0 { Color32::GREEN } else { Color32::RED };
                                            ui.label(RichText::new(format!("{:.3}", val)).color(color));
                                        } else {
                                            ui.label("-");
                                        }
                                    }
                                    DataType::Float32 => {
                                        let v = col.f32().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0.0 { Color32::GREEN } else { Color32::RED };
                                            ui.label(RichText::new(format!("{:.3}", val)).color(color));
                                        } else {
                                            ui.label("-");
                                        }
                                    }
                                    DataType::String => {
                                        let v = col.str().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            ui.label(val);
                                        } else {
                                            ui.label("-");
                                        }
                                    }
                                    _ => {
                                        ui.label("?");
                                    }
                                }
                            });
                        }
                    });
                }
            });
    })
    .response
}
