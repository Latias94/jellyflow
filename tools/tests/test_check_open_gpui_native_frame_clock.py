from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT_PATH = (
    Path(__file__).resolve().parents[1] / "check_open_gpui_native_frame_clock.py"
)
SPEC = importlib.util.spec_from_file_location(
    "check_open_gpui_native_frame_clock", SCRIPT_PATH
)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
frame_clock = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = frame_clock
SPEC.loader.exec_module(frame_clock)


def post_release_sequence() -> str:
    frames = []
    for index in range(frame_clock.REQUIRED_POST_RELEASE_FRAMES):
        frames.append(
            ":".join(
                map(
                    str,
                    (
                        index,
                        "Idle",
                        21.5,
                        -2.0,
                        1.0,
                        5,
                        6,
                        5,
                        3,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ),
                )
            )
        )
    return ";".join(frames)


def changed_sequence(frame_index: int, field_index: int, value: object) -> str:
    frames = [frame.split(":") for frame in post_release_sequence().split(";")]
    frames[frame_index][field_index] = str(value)
    return ";".join(":".join(frame) for frame in frames)


def marker(**overrides: object) -> str:
    fields: dict[str, object] = {
        "status": "pass",
        "test_support": "false",
        "native_frame_request": "host-native-frame",
        "post_release_stable": "true",
        "post_release_settle_frames": 2,
        "redraw_request_count": 7,
        "redraw_driver_active": "false",
        "post_release_frames": 5,
        "post_release_sequence": post_release_sequence(),
        "callback_entries": 4,
        "callback_registrations": 5,
        "frame_requests": 5,
        "pan_samples": 3,
        "pan_delta_x": 21.5,
    }
    fields.update(overrides)
    values = " ".join(f"{key}={value}" for key, value in fields.items())
    return f"{frame_clock.MARKER_PREFIX}{values}\n"


class NativeFrameClockEvidenceTests(unittest.TestCase):
    def test_valid_marker_proves_native_callbacks_and_continuous_pan(self) -> None:
        evidence = frame_clock.parse_pass_marker(marker())

        self.assertEqual(evidence.callback_entries, 4)
        self.assertEqual(evidence.pan_samples, 3)
        self.assertEqual(evidence.pan_delta_x, 21.5)
        self.assertEqual(evidence.settle_frames, 2)
        self.assertEqual(evidence.redraw_requests, 7)
        self.assertEqual(len(evidence.post_release_frames), 5)
        self.assertTrue(
            all(frame.phase == "Idle" for frame in evidence.post_release_frames)
        )

    def test_test_support_build_is_rejected(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "non-test-support"):
            frame_clock.parse_pass_marker(marker(test_support="true"))

    def test_single_pan_sample_is_not_continuous_evidence(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "three observed native frames"):
            frame_clock.parse_pass_marker(marker(pan_samples=1))

    def test_post_release_callback_growth_is_rejected(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "callback entries changed"):
            frame_clock.parse_pass_marker(
                marker(post_release_sequence=changed_sequence(2, 5, 6))
            )

    def test_post_release_frame_request_growth_is_rejected(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "frame requests changed"):
            frame_clock.parse_pass_marker(
                marker(post_release_sequence=changed_sequence(3, 7, 6))
            )

    def test_pending_token_query_and_native_wake_are_rejected(self) -> None:
        for field_index, message in (
            (9, "queries"),
            (12, "tick-token"),
            (16, "native-tick-wake"),
            (17, "native-query-wake"),
        ):
            with self.subTest(field_index=field_index):
                with self.assertRaisesRegex(RuntimeError, message):
                    frame_clock.parse_pass_marker(
                        marker(
                            post_release_sequence=changed_sequence(
                                1, field_index, 1
                            )
                        )
                    )

    def test_false_stability_claim_is_rejected(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "stop after drag release"):
            frame_clock.parse_pass_marker(marker(post_release_stable="false"))

    def test_missing_or_unbounded_settle_frames_are_rejected(self) -> None:
        for settle_frames in (0, frame_clock.MAX_POST_RELEASE_SETTLE_FRAMES + 1):
            with self.subTest(settle_frames=settle_frames):
                with self.assertRaisesRegex(RuntimeError, "settle count"):
                    frame_clock.parse_pass_marker(
                        marker(post_release_settle_frames=settle_frames)
                    )

    def test_missing_unbounded_or_active_redraw_driver_is_rejected(self) -> None:
        for redraw_requests in (0, frame_clock.MAX_POST_RELEASE_REDRAW_REQUESTS + 1):
            with self.subTest(redraw_requests=redraw_requests):
                with self.assertRaisesRegex(RuntimeError, "redraw request count"):
                    frame_clock.parse_pass_marker(
                        marker(redraw_request_count=redraw_requests)
                    )
        with self.assertRaisesRegex(RuntimeError, "remained active"):
            frame_clock.parse_pass_marker(marker(redraw_driver_active="true"))

    def test_incomplete_sequence_and_minimal_marker_are_rejected(self) -> None:
        four_frames = ";".join(post_release_sequence().split(";")[:4])
        with self.assertRaisesRegex(RuntimeError, "declared post-release frame count"):
            frame_clock.parse_pass_marker(marker(post_release_sequence=four_frames))

        minimal = (
            f"{frame_clock.MARKER_PREFIX}status=pass test_support=false "
            "post_release_stable=true\n"
        )
        with self.assertRaisesRegex(RuntimeError, "marker fields mismatch"):
            frame_clock.parse_pass_marker(minimal)

    def test_duplicate_result_markers_are_rejected(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "exactly one"):
            frame_clock.parse_pass_marker(marker() + marker())

    def test_callback_entries_cannot_exceed_registrations(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "exceeds callback registrations"):
            frame_clock.parse_pass_marker(
                marker(callback_entries=6, callback_registrations=5)
            )


if __name__ == "__main__":
    unittest.main()
