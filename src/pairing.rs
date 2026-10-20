//! The link and QR code Insulator Mobile pairs with: the daemon's WebSocket
//! address and token in one `insulator://pair` URL, so the phone takes both
//! from a scan or a paste instead of having them typed in.

use qrcode::{Color, QrCode};

/// `insulator://pair?url=ws://host:port/v1&token=…`, the format the mobile
/// app parses from a scanned code, a pasted link or an opened URL.
pub fn pairing_link(host: &str, port: u16, token: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("url", &format!("ws://{host}:{port}/v1"))
        .append_pair("token", token)
        .finish();
    format!("insulator://pair?{query}")
}

/// A QR code's modules, row by row, dark as `true`. `None` only when the link
/// is too long for any QR version.
pub struct QrModules {
    pub width: usize,
    pub dark: Vec<bool>,
}

impl QrModules {
    pub fn encode(text: &str) -> Option<Self> {
        let code = QrCode::new(text.as_bytes()).ok()?;
        Some(Self {
            width: code.width(),
            dark: code
                .to_colors()
                .into_iter()
                .map(|color| color == Color::Dark)
                .collect(),
        })
    }

    /// Each row's dark runs as `(start, length)`, so a row draws as a few
    /// rectangles rather than one per module.
    pub fn row_runs(&self, row: usize) -> Vec<(usize, usize)> {
        let cells = &self.dark[row * self.width..(row + 1) * self.width];
        let mut runs = Vec::new();
        let mut column = 0;
        while column < cells.len() {
            if !cells[column] {
                column += 1;
                continue;
            }
            let start = column;
            while column < cells.len() && cells[column] {
                column += 1;
            }
            runs.push((start, column - start));
        }
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_link_carries_an_encoded_url_and_token() {
        let link = pairing_link("100.64.0.1", 34123, "a b&c");
        assert_eq!(
            link,
            "insulator://pair?url=ws%3A%2F%2F100.64.0.1%3A34123%2Fv1&token=a+b%26c"
        );
    }

    #[test]
    fn row_runs_cover_exactly_the_dark_modules() {
        let modules = QrModules::encode(&pairing_link("100.64.0.1", 34123, "secret")).unwrap();
        for row in 0..modules.width {
            let mut drawn = vec![false; modules.width];
            for (start, length) in modules.row_runs(row) {
                drawn[start..start + length].fill(true);
            }
            assert_eq!(
                drawn,
                modules.dark[row * modules.width..(row + 1) * modules.width]
            );
        }
    }
}
