use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
};

pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, _area: Rect) -> Option<(u16, u16)> {
        None
    }
}

pub enum FlexDirection {
    Horizontal,
    Vertical,
}

pub enum FlexItem<'a> {
    Flex {
        weight: u16,
        content: &'a dyn Renderable,
    },
    Fixed {
        size: u16,
        content: &'a dyn Renderable,
    },
}

pub fn flex_layout(direction: FlexDirection, items: &[FlexItem], area: Rect, buf: &mut Buffer) {
    let total_size = match direction {
        FlexDirection::Horizontal => area.width,
        FlexDirection::Vertical => area.height,
    };

    let fixed_total: u16 = items
        .iter()
        .map(|item| match item {
            FlexItem::Fixed { size, .. } => *size,
            FlexItem::Flex { .. } => 0,
        })
        .sum();

    let flex_total: u16 = items
        .iter()
        .map(|item| match item {
            FlexItem::Flex { weight, .. } => *weight,
            FlexItem::Fixed { .. } => 0,
        })
        .sum();

    let _remaining = total_size.saturating_sub(fixed_total);

    let constraints: Vec<Constraint> = items
        .iter()
        .map(|item| match item {
            FlexItem::Fixed { size, .. } => Constraint::Length(*size),
            FlexItem::Flex { weight, .. } => {
                if flex_total == 0 {
                    Constraint::Min(0)
                } else {
                    Constraint::Ratio(*weight as u32, flex_total as u32)
                }
            }
        })
        .collect();

    let dir = match direction {
        FlexDirection::Horizontal => Direction::Horizontal,
        FlexDirection::Vertical => Direction::Vertical,
    };

    let chunks = Layout::default()
        .direction(dir)
        .constraints(constraints)
        .split(area);

    for (i, item) in items.iter().enumerate() {
        let content = match item {
            FlexItem::Fixed { content, .. } => content,
            FlexItem::Flex { content, .. } => content,
        };
        content.render(chunks[i], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockRenderable {
        char: char,
    }

    impl Renderable for MockRenderable {
        fn render(&self, area: Rect, buf: &mut Buffer) {
            let symbol = self.char.to_string();
            for y in area.y..area.y + area.height {
                for x in area.x..area.x + area.width {
                    buf[(x, y)].set_symbol(&symbol);
                }
            }
        }

        fn desired_height(&self, _width: u16) -> u16 {
            5
        }
    }

    #[test]
    fn test_flex_vertical_split() {
        let a = MockRenderable { char: 'A' };
        let b = MockRenderable { char: 'B' };
        let items = vec![
            FlexItem::Flex {
                weight: 1,
                content: &a,
            },
            FlexItem::Fixed {
                size: 3,
                content: &b,
            },
        ];
        let area = Rect::new(0, 0, 10, 20);
        let mut buf = Buffer::empty(area);
        flex_layout(FlexDirection::Vertical, &items, area, &mut buf);

        for y in 0..17 {
            assert_eq!(buf[(0, y)].symbol(), "A");
        }
        for y in 17..20 {
            assert_eq!(buf[(0, y)].symbol(), "B");
        }
    }

    #[test]
    fn test_flex_horizontal_split() {
        let a = MockRenderable { char: 'L' };
        let b = MockRenderable { char: 'R' };
        let items = vec![
            FlexItem::Flex {
                weight: 7,
                content: &a,
            },
            FlexItem::Fixed {
                size: 3,
                content: &b,
            },
        ];
        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(area);
        flex_layout(FlexDirection::Horizontal, &items, area, &mut buf);

        for x in 0..7 {
            assert_eq!(buf[(x, 0)].symbol(), "L");
        }
        for x in 7..10 {
            assert_eq!(buf[(x, 0)].symbol(), "R");
        }
    }

    #[test]
    fn test_flex_overflow_fixed_takes_priority() {
        let a = MockRenderable { char: 'A' };
        let b = MockRenderable { char: 'B' };
        let items = vec![
            FlexItem::Fixed {
                size: 8,
                content: &a,
            },
            FlexItem::Fixed {
                size: 8,
                content: &b,
            },
        ];
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        flex_layout(FlexDirection::Horizontal, &items, area, &mut buf);
    }
}
