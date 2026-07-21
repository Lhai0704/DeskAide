use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

pub fn assistant_position(avatar: Rect, assistant: Size, work_area: Rect, gap: i32) -> (i32, i32) {
    let centered_y = avatar.y as i64 + (avatar.height as i64 - assistant.height as i64) / 2;
    let centered_x = avatar.x as i64 + (avatar.width as i64 - assistant.width as i64) / 2;
    let candidates = [
        (
            avatar.x as i64 + avatar.width as i64 + gap as i64,
            centered_y,
        ),
        (
            avatar.x as i64 - assistant.width as i64 - gap as i64,
            centered_y,
        ),
        (
            centered_x,
            avatar.y as i64 + avatar.height as i64 + gap as i64,
        ),
        (
            centered_x,
            avatar.y as i64 - assistant.height as i64 - gap as i64,
        ),
    ];

    for candidate in candidates {
        if fits(candidate, assistant, work_area) {
            return (candidate.0 as i32, candidate.1 as i32);
        }
    }

    (
        clamp_axis(
            candidates[0].0,
            work_area.x as i64,
            work_area.width,
            assistant.width,
        ),
        clamp_axis(
            candidates[0].1,
            work_area.y as i64,
            work_area.height,
            assistant.height,
        ),
    )
}

pub fn clamp_rect(position: (i32, i32), size: Size, work_area: Rect) -> (i32, i32) {
    (
        clamp_axis(
            position.0 as i64,
            work_area.x as i64,
            work_area.width,
            size.width,
        ),
        clamp_axis(
            position.1 as i64,
            work_area.y as i64,
            work_area.height,
            size.height,
        ),
    )
}

fn fits(position: (i64, i64), size: Size, work_area: Rect) -> bool {
    let right = work_area.x as i64 + work_area.width as i64;
    let bottom = work_area.y as i64 + work_area.height as i64;
    position.0 >= work_area.x as i64
        && position.1 >= work_area.y as i64
        && position.0 + size.width as i64 <= right
        && position.1 + size.height as i64 <= bottom
}

fn clamp_axis(value: i64, area_start: i64, area_length: u32, item_length: u32) -> i32 {
    if item_length >= area_length {
        return area_start as i32;
    }
    let max = area_start + area_length as i64 - item_length as i64;
    value.clamp(area_start, max) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    const ASSISTANT: Size = Size {
        width: 420,
        height: 460,
    };

    #[test]
    fn prefers_the_right_side() {
        let result = assistant_position(
            Rect {
                x: 500,
                y: 300,
                width: 160,
                height: 160,
            },
            ASSISTANT,
            SCREEN,
            12,
        );
        assert_eq!(result, (672, 150));
    }

    #[test]
    fn falls_back_to_the_left_at_the_right_edge() {
        let result = assistant_position(
            Rect {
                x: 1760,
                y: 300,
                width: 160,
                height: 160,
            },
            ASSISTANT,
            SCREEN,
            12,
        );
        assert_eq!(result, (1328, 150));
    }

    #[test]
    fn handles_negative_monitor_coordinates() {
        let work_area = Rect {
            x: -1920,
            y: -120,
            width: 1920,
            height: 1080,
        };
        let result = assistant_position(
            Rect {
                x: -180,
                y: 120,
                width: 160,
                height: 160,
            },
            ASSISTANT,
            work_area,
            12,
        );
        assert_eq!(result, (-612, -30));
    }

    #[test]
    fn oversized_items_anchor_to_the_work_area() {
        assert_eq!(
            clamp_rect(
                (200, 200),
                Size {
                    width: 2_000,
                    height: 2_000,
                },
                SCREEN,
            ),
            (0, 0),
        );
    }

    #[test]
    fn clamps_an_offscreen_saved_position() {
        assert_eq!(clamp_rect((3_000, -500), ASSISTANT, SCREEN), (1500, 0));
    }
}
