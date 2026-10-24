use windows::Win32::Foundation::{POINT, RECT};

pub fn scale(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi.max(96) as i64 + 48) / 96) as i32
}

/// Prefer the cursor's right side, flip at the right edge, and stay inside the
/// cursor monitor's work area (which may have negative virtual-screen coordinates).
pub fn indicator_rect(cursor: POINT, work: RECT, dpi: u32, y_offset: i32) -> RECT {
    let size = scale(24, dpi)
        .min((work.right - work.left).max(1))
        .min((work.bottom - work.top).max(1));
    let gap = scale(20, dpi);
    let preferred_x = cursor.x + gap;
    let x = if preferred_x + size <= work.right {
        preferred_x
    } else {
        cursor.x - gap - size
    }
    .clamp(work.left, (work.right - size).max(work.left));
    let offset = scale(y_offset.abs(), dpi) * y_offset.signum();
    let y = (cursor.y + offset).clamp(work.top, (work.bottom - size).max(work.top));
    RECT {
        left: x,
        top: y,
        right: x + size,
        bottom: y + size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_and_font_scale_together_at_common_display_scales() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 3840,
            bottom: 2160,
        };
        for (dpi, size, font) in [(96, 24, 15), (120, 30, 19), (144, 36, 23), (192, 48, 30)] {
            let rect = indicator_rect(POINT { x: 100, y: 100 }, work, dpi, 12);
            assert_eq!(rect.right - rect.left, size);
            assert_eq!(scale(15, dpi), font);
        }
    }

    #[test]
    fn edges_and_negative_monitor_coordinates_keep_entire_indicator_visible() {
        let work = RECT {
            left: -1920,
            top: -1080,
            right: 0,
            bottom: 0,
        };
        for dpi in [96, 120, 144, 192, 288] {
            for point in [(-1920, -1080), (-1, -1080), (-1920, -1), (-1, -1)] {
                for offset in [-12, 12] {
                    let rect = indicator_rect(
                        POINT {
                            x: point.0,
                            y: point.1,
                        },
                        work,
                        dpi,
                        offset,
                    );
                    assert!(rect.left >= work.left && rect.top >= work.top);
                    assert!(rect.right <= work.right && rect.bottom <= work.bottom);
                }
            }
            let right_edge = indicator_rect(POINT { x: -1, y: -500 }, work, dpi, 12);
            assert!(
                right_edge.right < -1,
                "badge should flip to the cursor's left"
            );
        }
    }

    #[test]
    fn work_area_smaller_than_badge_does_not_panic_or_overflow() {
        let work = RECT {
            left: 10,
            top: 20,
            right: 20,
            bottom: 25,
        };
        let rect = indicator_rect(POINT { x: 100, y: 100 }, work, 192, 12);
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (15, 20, 20, 25)
        );
    }
}
