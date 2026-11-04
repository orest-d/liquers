use egui::{Color32, RichText};
use egui_extras::{Column, TableBuilder};
use polars::prelude::*;

/// `df` sorted by `sort_state` (column index, ascending), or unchanged when there is no sort
/// state, the index is out of range, or the sort fails. The rows a table shows are this frame's.
fn sorted_frame(df: &DataFrame, sort_state: Option<(usize, bool)>) -> DataFrame {
    let Some((col_idx, ascending)) = sort_state else {
        return df.clone();
    };
    let Some(col_name) = df.get_column_names().get(col_idx).map(|s| s.to_string()) else {
        return df.clone();
    };
    let options = polars::prelude::SortMultipleOptions {
        descending: vec![!ascending],
        ..Default::default()
    };
    match df.sort([col_name.as_str()], options) {
        Ok(sorted_df) => sorted_df,
        Err(_) => {
            eprintln!("Failed to sort dataframe by column: {}", col_name);
            df.clone()
        }
    }
}

/// Display a polars DataFrame as a sortable table in egui.
/// Data types are indicated by color:
/// - Positive numbers: green
/// - Negative numbers: red
/// - Strings: default color
pub fn display_polars_dataframe(
    ui: &mut egui::Ui,
    df: &DataFrame,
    sort_state: &mut Option<(usize, bool)>,
) -> egui::Response {
    let df = sorted_frame(df, *sort_state);
    let columns = df.columns();
    let nrows = df.height();
    let ncols = columns.len();
    let col_names: Vec<_> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    let missing_icon = |ui: &mut egui::Ui| {
        // Show a yellow 'ⁿ̷ₐ' as missing value indicator
        ui.label(RichText::new("ⁿ̷ₐ").color(Color32::ORANGE));
    };

    let light_green = Color32::from_rgb(144, 238, 144); // light green for unsigned integers
    let cyan = Color32::from_rgb(0, 255, 255); // cyan for temporal types

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
                            eprintln!("Sorting by column: {} {:?}", name, *sort_state);
                            match *sort_state {
                                Some((idx, asc)) if idx == i => *sort_state = Some((i, !asc)),
                                _ => *sort_state = Some((i, true)),
                            }
                            eprintln!("Sorting changed: {} {:?}", name, *sort_state);
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
                                            let color = if val >= 0 {
                                                Color32::GREEN
                                            } else {
                                                Color32::RED
                                            };
                                            ui.label(RichText::new(val.to_string()).color(color));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Int32 => {
                                        let v = col.i32().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0 {
                                                Color32::GREEN
                                            } else {
                                                Color32::RED
                                            };
                                            ui.label(RichText::new(val.to_string()).color(color));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Float64 => {
                                        let v = col.f64().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0.0 {
                                                Color32::GREEN
                                            } else {
                                                Color32::RED
                                            };
                                            ui.label(
                                                RichText::new(format!("{:.3}", val)).color(color),
                                            );
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Float32 => {
                                        let v = col.f32().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0.0 {
                                                Color32::GREEN
                                            } else {
                                                Color32::RED
                                            };
                                            ui.label(
                                                RichText::new(format!("{:.3}", val)).color(color),
                                            );
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::String => {
                                        let v = col.str().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            ui.label(val);
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Boolean => {
                                        let v = col.bool().unwrap().get(row_idx);
                                        match v {
                                            Some(true) => {
                                                ui.colored_label(Color32::GREEN, "true");
                                            }
                                            Some(false) => {
                                                ui.colored_label(Color32::RED, "false");
                                            }
                                            None => {
                                                missing_icon(ui);
                                            }
                                        }
                                    }
                                    DataType::UInt8 => {
                                        let v = col.u8().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            ui.label(
                                                RichText::new(val.to_string()).color(light_green),
                                            );
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::UInt16 => {
                                        let v = col.u16().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            ui.label(
                                                RichText::new(val.to_string()).color(light_green),
                                            );
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::UInt32 => {
                                        let v = col.u32().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            ui.label(
                                                RichText::new(val.to_string()).color(light_green),
                                            );
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::UInt64 => {
                                        let v = col.u64().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            ui.label(
                                                RichText::new(val.to_string()).color(light_green),
                                            );
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Int8 => {
                                        let v = col.i8().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0 {
                                                Color32::GREEN
                                            } else {
                                                Color32::RED
                                            };
                                            ui.label(RichText::new(val.to_string()).color(color));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Int16 => {
                                        let v = col.i16().unwrap().get(row_idx);
                                        if let Some(val) = v {
                                            let color = if val >= 0 {
                                                Color32::GREEN
                                            } else {
                                                Color32::RED
                                            };
                                            ui.label(RichText::new(val.to_string()).color(color));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Binary | DataType::BinaryOffset => {
                                        // Show length of binary data
                                        let v = col.binary().ok().and_then(|s| s.get(row_idx));
                                        if let Some(bytes) = v {
                                            ui.label(format!("[{} bytes]", bytes.len()));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Date => {
                                        let v = col.date().unwrap().to_string("%Y-%m-%d").unwrap();
                                        if let Some(val) = v.get(row_idx) {
                                            ui.label(RichText::new(val).color(cyan));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Datetime(_time_unit, _time_zone) => {
                                        let v = col
                                            .datetime()
                                            .unwrap()
                                            .to_string("%Y-%m-%d %H:%M:%S")
                                            .unwrap();
                                        if let Some(val) = v.get(row_idx) {
                                            ui.label(RichText::new(val).color(cyan));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Duration(_time_unit) => {
                                        let v = col.duration().unwrap().phys.get(row_idx);
                                        if let Some(val) = v {
                                            ui.label(format!("{val}"));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::List(_) => {
                                        // Show length of list
                                        let v = col.list().unwrap().get(row_idx);
                                        if let Some(list) = v {
                                            ui.label(format!("[list; len={}]", list.len()));
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Null => {
                                        ui.label("null");
                                    }
                                    DataType::Categorical(_, _) | DataType::Enum(_, _) => {
                                        // Show as string if possible
                                        let v = col.str().ok().and_then(|s| s.get(row_idx));
                                        if let Some(val) = v {
                                            ui.label(val);
                                        } else {
                                            missing_icon(ui);
                                        }
                                    }
                                    DataType::Int128 => {
                                        // Not supported in polars 0.51.0, show placeholder
                                        ui.label("[int128]");
                                    }
                                    DataType::Time => {
                                        // Not supported in polars 0.51.0, show placeholder
                                        ui.label("[time]");
                                    }
                                    DataType::Struct(_) => {
                                        // Show as struct
                                        ui.label("[struct]");
                                    }
                                    _ => {
                                        ui.label("[unknown]");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_frame_orders_rows() {
        let df = df!("n" => [3i64, 1, 2], "s" => ["c", "a", "b"]).unwrap();
        let asc = sorted_frame(&df, Some((0, true)));
        let n: Vec<_> = asc.column("n").unwrap().i64().unwrap().iter().collect();
        assert_eq!(n, vec![Some(1), Some(2), Some(3)]);
        let s: Vec<_> = asc.column("s").unwrap().str().unwrap().iter().collect();
        assert_eq!(s, vec![Some("a"), Some("b"), Some("c")]);

        let desc = sorted_frame(&df, Some((1, false)));
        let n: Vec<_> = desc.column("n").unwrap().i64().unwrap().iter().collect();
        assert_eq!(n, vec![Some(3), Some(2), Some(1)]);

        assert!(sorted_frame(&df, None).equals(&df));
        assert!(sorted_frame(&df, Some((9, true))).equals(&df));
    }
}
