#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    pub char_width: f64,
    pub line_height: f64,
    pub gutter_width: f64,
    pub padding_x: f64,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            char_width: 8.4,
            line_height: 22.4,
            gutter_width: 40.0,
            padding_x: 8.0,
        }
    }
}
