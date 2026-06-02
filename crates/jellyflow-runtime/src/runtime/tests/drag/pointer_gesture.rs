use crate::runtime::drag::{
    PointerGestureClaim, PointerGestureClaimInput, resolve_pointer_gesture_claim,
};
use jellyflow_core::core::CanvasPoint;

#[test]
fn pointer_gesture_claim_prioritizes_selection_before_node_drag() {
    assert_eq!(
        resolve_pointer_gesture_claim(PointerGestureClaimInput::new(
            CanvasPoint { x: 2.0, y: 0.0 },
            true,
            false,
            8.0,
            0.0,
        )),
        PointerGestureClaim::Selection
    );

    assert_eq!(
        resolve_pointer_gesture_claim(PointerGestureClaimInput::new(
            CanvasPoint::default(),
            false,
            true,
            8.0,
            0.0,
        )),
        PointerGestureClaim::Selection
    );
}

#[test]
fn pointer_gesture_claim_falls_back_to_node_drag_threshold() {
    assert_eq!(
        resolve_pointer_gesture_claim(PointerGestureClaimInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            false,
            false,
            8.0,
            4.99,
        )),
        PointerGestureClaim::NodeDrag
    );

    assert_eq!(
        resolve_pointer_gesture_claim(PointerGestureClaimInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            false,
            false,
            8.0,
            5.0,
        )),
        PointerGestureClaim::None
    );
}
