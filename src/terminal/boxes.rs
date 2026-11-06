use gpui::{Bounds, Hsla, Pixels, Point, Size, point, px, size};

fn lines(c: char) -> Option<(bool, bool, bool, bool, bool)> {
    let (left, right, up, down, heavy) = match c {
        '─' => (true, true, false, false, false),
        '│' => (false, false, true, true, false),
        '┌' | '╭' => (false, true, false, true, false),
        '┐' | '╮' => (true, false, false, true, false),
        '└' | '╰' => (false, true, true, false, false),
        '┘' | '╯' => (true, false, true, false, false),
        '├' => (false, true, true, true, false),
        '┤' => (true, false, true, true, false),
        '┬' => (true, true, false, true, false),
        '┴' => (true, true, true, false, false),
        '┼' => (true, true, true, true, false),
        '━' | '═' => (true, true, false, false, true),
        '┃' | '║' => (false, false, true, true, true),
        '┏' | '╔' => (false, true, false, true, true),
        '┓' | '╗' => (true, false, false, true, true),
        '┗' | '╚' => (false, true, true, false, true),
        '┛' | '╝' => (true, false, true, false, true),
        '┣' | '╠' => (false, true, true, true, true),
        '┫' | '╣' => (true, false, true, true, true),
        '┳' | '╦' => (true, true, false, true, true),
        '┻' | '╩' => (true, true, true, false, true),
        '╋' | '╬' => (true, true, true, true, true),
        _ => return None,
    };
    Some((left, right, up, down, heavy))
}

fn block(c: char) -> Option<(f32, f32, f32, f32, f32)> {
    let eighth = |count: u32| count as f32 / 8.0;
    Some(match c {
        '█' => (0.0, 0.0, 1.0, 1.0, 1.0),
        '▀' => (0.0, 0.0, 1.0, 0.5, 1.0),
        '▐' => (0.5, 0.0, 1.0, 1.0, 1.0),
        '░' => (0.0, 0.0, 1.0, 1.0, 0.25),
        '▒' => (0.0, 0.0, 1.0, 1.0, 0.5),
        '▓' => (0.0, 0.0, 1.0, 1.0, 0.75),
        '▔' => (0.0, 0.0, 1.0, eighth(1), 1.0),
        '▕' => (1.0 - eighth(1), 0.0, 1.0, 1.0, 1.0),
        '\u{2581}'..='\u{2587}' => (0.0, 1.0 - eighth(c as u32 - 0x2580), 1.0, 1.0, 1.0),
        '\u{2589}'..='\u{258F}' => (0.0, 0.0, eighth(0x2590 - c as u32), 1.0, 1.0),
        _ => return None,
    })
}

fn quadrants(c: char) -> Option<[bool; 4]> {
    Some(match c {
        '▖' => [false, false, true, false],
        '▗' => [false, false, false, true],
        '▘' => [true, false, false, false],
        '▙' => [true, false, true, true],
        '▚' => [true, false, false, true],
        '▛' => [true, true, true, false],
        '▜' => [true, true, false, true],
        '▝' => [false, true, false, false],
        '▞' => [false, true, true, false],
        '▟' => [false, true, true, true],
        _ => return None,
    })
}

pub fn is_drawn(c: char) -> bool {
    lines(c).is_some() || block(c).is_some() || quadrants(c).is_some()
}

pub fn rects(c: char, origin: Point<Pixels>, cell: Size<Pixels>, color: Hsla) -> Vec<(Bounds<Pixels>, Hsla)> {
    let (width, height) = (f32::from(cell.width), f32::from(cell.height));
    if let Some((x0, y0, x1, y1, alpha)) = block(c) {
        let start = point(origin.x + px((x0 * width).round()), origin.y + px((y0 * height).round()));
        let extent = size(px(((x1 - x0) * width).round().max(1.0)), px(((y1 - y0) * height).round().max(1.0)));
        return vec![(Bounds::new(start, extent), color.opacity(alpha))];
    }
    if let Some(filled) = quadrants(c) {
        let (middle_x, middle_y) = ((width / 2.0).round(), (height / 2.0).round());
        let columns = [(0.0, middle_x), (middle_x, width.ceil() - middle_x)];
        let rows = [(0.0, middle_y), (middle_y, height.ceil() - middle_y)];
        return (0..filled.len())
            .filter(|index| filled[*index])
            .map(|index| {
                let ((x, w), (y, h)) = (columns[index % 2], rows[index / 2]);
                (Bounds::new(point(origin.x + px(x), origin.y + px(y)), size(px(w), px(h))), color)
            })
            .collect();
    }
    let Some((left, right, up, down, heavy)) = lines(c) else { return Vec::new() };
    let thin = (height / 16.0).round().max(1.0);
    let thickness = if heavy { thin * 2.0 } else { thin };
    let (middle_x, middle_y) = (((width - thickness) / 2.0).round(), ((height - thickness) / 2.0).round());
    let mut out = Vec::new();
    let mut push = |x: f32, y: f32, w: f32, h: f32| {
        out.push((Bounds::new(point(origin.x + px(x), origin.y + px(y)), size(px(w), px(h))), color));
    };
    if left {
        push(0.0, middle_y, middle_x + thickness, thickness);
    }
    if right {
        push(middle_x, middle_y, width - middle_x, thickness);
    }
    if up {
        push(middle_x, 0.0, thickness, middle_y + thickness);
    }
    if down {
        push(middle_x, middle_y, thickness, height - middle_y);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_expected_shapes() {
        let cell = size(px(8.0), px(16.0));
        let origin = point(px(0.0), px(0.0));
        let color: Hsla = gpui::rgb(0xFFFFFF).into();
        assert_eq!(rects('─', origin, cell, color).len(), 2);
        assert_eq!(rects('│', origin, cell, color).len(), 2);
        assert_eq!(rects('┼', origin, cell, color).len(), 4);
        let full = rects('█', origin, cell, color);
        assert_eq!(full[0].0.size, cell);
        let half = rects('▄', origin, cell, color);
        assert_eq!(half[0].0.origin.y, px(8.0));
        let left = rects('▌', origin, cell, color);
        assert_eq!(left[0].0.size.width, px(4.0));
        let corner = rects('▛', origin, cell, color);
        assert_eq!(corner.len(), 3);
        assert_eq!(corner[1].0, Bounds::new(point(px(4.0), px(0.0)), size(px(4.0), px(8.0))));
        assert_eq!(corner[2].0, Bounds::new(point(px(0.0), px(8.0)), size(px(4.0), px(8.0))));
        assert!(is_drawn('▝'));
        assert!(!is_drawn('a'));
    }
}
