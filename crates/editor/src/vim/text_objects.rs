use std::ops::Range;

use rope::TextRope;

use crate::editor::motions::{self, MotionRangeBehavior};

use super::state::RepeatTarget;

pub(crate) fn resolve_repeat_target_range(
    rope: &TextRope,
    cursor_byte_offset: usize,
    target: RepeatTarget,
    is_change: bool,
) -> Option<Range<usize>> {
    match target {
        RepeatTarget::Motion(motion) => motions::resolve_motion_range(
            rope,
            cursor_byte_offset,
            motion,
            if is_change {
                MotionRangeBehavior::Change
            } else {
                MotionRangeBehavior::Default
            },
        ),
        RepeatTarget::TextObject(modifier, target) => {
            motions::resolve_text_object_range(rope, cursor_byte_offset, modifier, target)
        }
        RepeatTarget::Line => None,
    }
}
