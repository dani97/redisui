use ratatui::layout::{Constraint, Layout, Rect};

pub const TYPES_PANE_WIDTH: u16 = 26;

pub struct Panes {
    pub types: Rect,
    pub keys: Rect,
    pub value: Rect,
    pub detail: Rect,
    pub status: Rect,
}

pub fn panes(area: Rect) -> Panes {
    let [body, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    let [left, right] =
        Layout::horizontal([Constraint::Length(TYPES_PANE_WIDTH), Constraint::Fill(1)]).areas(body);
    let [keys, value] = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(right);
    let [types, detail] = Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(left);

    Panes {
        types,
        keys,
        value,
        detail,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_into_four_panes_plus_status() {
        let rects = panes(Rect::new(0, 0, 100, 40));

        assert_eq!(rects.types.x, 0);
        assert_eq!(rects.types.width, TYPES_PANE_WIDTH);
        assert_eq!(rects.keys.x, TYPES_PANE_WIDTH);
        assert_eq!(rects.keys.y, 0);
        assert_eq!(rects.value.y, rects.keys.y + rects.keys.height);
        assert_eq!(rects.detail.y, rects.types.y + rects.types.height);
        assert_eq!(rects.status.height, 1);
        assert_eq!(rects.status.y, 39);
        assert!(rects.keys.right() <= 100);
    }
}
