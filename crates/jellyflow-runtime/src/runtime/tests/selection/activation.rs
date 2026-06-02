use crate::runtime::selection::{
    SelectionDragActivationInput, SelectionPointerClaim, SelectionPointerClaimInput,
    resolve_selection_pointer_claim, selection_drag_threshold_met,
};
use jellyflow_core::core::CanvasPoint;

#[test]
fn selection_drag_threshold_uses_xyflow_screen_distance_semantics() {
    assert!(
        !selection_drag_threshold_met(SelectionDragActivationInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            5.0,
            false,
        )),
        "distance equal to pane click distance does not start selection"
    );
    assert!(
        selection_drag_threshold_met(SelectionDragActivationInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            4.99,
            false,
        )),
        "distance greater than pane click distance starts selection"
    );
    assert!(
        !selection_drag_threshold_met(SelectionDragActivationInput::new(
            CanvasPoint::default(),
            8.0,
            true,
        )),
        "selection key bypasses pane distance but still requires positive movement"
    );
    assert!(
        selection_drag_threshold_met(SelectionDragActivationInput::new(
            CanvasPoint { x: 0.1, y: 0.0 },
            8.0,
            true,
        )),
        "selection key allows any positive movement"
    );
}

#[test]
fn selection_drag_threshold_normalizes_invalid_config_edges() {
    assert!(
        !selection_drag_threshold_met(SelectionDragActivationInput::new(
            CanvasPoint::default(),
            -1.0,
            false,
        )),
        "negative pane distance is treated as zero"
    );
    assert!(
        selection_drag_threshold_met(SelectionDragActivationInput::new(
            CanvasPoint { x: 0.1, y: 0.0 },
            -1.0,
            false,
        )),
        "positive movement exceeds a normalized zero threshold"
    );
    assert!(!selection_drag_threshold_met(
        SelectionDragActivationInput::new(CanvasPoint { x: 1.0, y: 0.0 }, f32::NAN, false),
    ));
    assert!(!selection_drag_threshold_met(
        SelectionDragActivationInput::new(
            CanvasPoint {
                x: f32::INFINITY,
                y: 0.0,
            },
            1.0,
            false,
        ),
    ));
}

#[test]
fn selection_pointer_claim_distinguishes_possible_and_active_marquee_ownership() {
    assert_eq!(
        resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
            CanvasPoint { x: 0.1, y: 0.0 },
            8.0,
            true,
            false,
        )),
        SelectionPointerClaim::SelectionMayClaimDrag
    );
    assert_eq!(
        resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
            CanvasPoint::default(),
            8.0,
            true,
            false,
        )),
        SelectionPointerClaim::Unclaimed
    );
    assert_eq!(
        resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
            CanvasPoint::default(),
            8.0,
            false,
            true,
        )),
        SelectionPointerClaim::SelectionOwnsDrag
    );
    assert_eq!(
        resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
            CanvasPoint { x: 0.1, y: 0.0 },
            0.0,
            true,
            false,
        )),
        SelectionPointerClaim::SelectionMayClaimDrag
    );
    assert_eq!(
        resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
            CanvasPoint { x: 0.1, y: 0.0 },
            0.0,
            false,
            false,
        )),
        SelectionPointerClaim::Unclaimed
    );
    assert_eq!(
        resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
            CanvasPoint { x: 0.1, y: 0.0 },
            0.0,
            true,
            true,
        )),
        SelectionPointerClaim::SelectionOwnsDrag
    );
}
