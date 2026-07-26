from __future__ import annotations

import binascii
import hashlib
import importlib.util
import json
import struct
import tempfile
import unittest
import zlib
from pathlib import Path
from typing import Any


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check_real_window_smoke_evidence.py"
SPEC = importlib.util.spec_from_file_location("check_real_window_smoke_evidence", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
evidence = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evidence)


INTERACTION_STATES = (
    "initial",
    "selected",
    "connection-preview",
    "connection-committed",
    "reconnect-selected",
    "reconnect-preview",
    "reconnect-committed",
    "search-open",
    "final",
)
LOD_STATES = ("lod-0.86", "lod-0.85", "lod-0.84")


def known_expected_names() -> set[str]:
    interaction_runs = (
        ("shader", "compact", "960x640", "0.5"),
        ("workflow", "compact", "960x640", "0.5"),
        ("shader", "compact", "960x640", "1"),
        ("workflow", "compact", "960x640", "1"),
        ("shader", "compact", "960x640", "2"),
        ("workflow", "compact", "960x640", "2"),
        ("shader", "normal", "1440x900", "1"),
        ("workflow", "normal", "1440x900", "1"),
    )
    names = {
        f"jellyflow-{demo}-{bounds}-{size}-z{zoom}-interaction-{state}.png"
        for demo, bounds, size, zoom in interaction_runs
        for state in INTERACTION_STATES
    }
    names.update(
        f"jellyflow-shader-normal-1440x900-z1-lod-threshold-{state}.png"
        for state in LOD_STATES
    )
    return names


def png_chunk(chunk_type: bytes, data: bytes, *, corrupt_crc: bool = False) -> bytes:
    crc = binascii.crc32(chunk_type)
    crc = binascii.crc32(data, crc) & 0xFFFFFFFF
    if corrupt_crc:
        crc ^= 1
    return struct.pack(">I", len(data)) + chunk_type + data + struct.pack(">I", crc)


def make_png(
    width: int,
    height: int,
    *,
    bit_depth: int = 1,
    color_type: int = 0,
    corrupt_ihdr_crc: bool = False,
    encoded_rows: int | None = None,
    split_idat: bool = False,
    truncate: int = 0,
) -> bytes:
    ihdr = struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, 0)
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color_type]
    row = b"\x00" + bytes((width * bit_depth * channels + 7) // 8)
    row_count = height if encoded_rows is None else encoded_rows
    compressed = zlib.compress(row * row_count)
    split_at = max(1, len(compressed) // 2)
    idat_chunks = (
        png_chunk(b"IDAT", compressed[:split_at])
        + png_chunk(b"IDAT", compressed[split_at:])
        if split_idat
        else png_chunk(b"IDAT", compressed)
    )
    contents = b"".join(
        (
            evidence.PNG_SIGNATURE,
            png_chunk(b"IHDR", ihdr, corrupt_crc=corrupt_ihdr_crc),
            idat_chunks,
            png_chunk(b"IEND", b""),
        )
    )
    return contents[:-truncate] if truncate else contents


def logical_size(name: str) -> tuple[int, int]:
    match = evidence.CAPTURE_SIZE_PATTERN.search(name)
    if match is None:
        raise AssertionError(f"fixture name has no size: {name}")
    return int(match.group("width")), int(match.group("height"))


def write_pngs(capture_dir: Path, names: set[str], *, scale: int = 1) -> dict[str, bytes]:
    payloads: dict[str, bytes] = {}
    for name in names:
        width, height = logical_size(name)
        payload = make_png(width * scale, height * scale)
        (capture_dir / name).write_bytes(payload)
        payloads[name] = payload
    return payloads


def manifest_line(name: str, payload: bytes, width: int, height: int) -> str:
    digest = hashlib.sha256(payload).hexdigest()
    return f"{name}\t{width}x{height}\tsha256={digest}"


def final_marker(spec: Any) -> str:
    return (
        "JELLYFLOW_REAL_WINDOW_SMOKE "
        f"status=pass demo={spec.demo} bounds={spec.bounds} "
        f"scenario={spec.scenario} target_zoom={spec.zoom} diagnostic=None"
    )


def valid_results_document() -> dict[str, object]:
    runs = []
    for spec in evidence.RUN_SPECS:
        marker = final_marker(spec)
        runs.append(
            {
                "run_id": spec.run_id,
                "config": spec.config(),
                "exit_code": 0,
                "timed_out": False,
                "marker_count": 1,
                "marker": marker,
                "stdout": marker + "\n",
                "stderr": "",
                "status": "pass",
                "errors": [],
            }
        )
    return {
        "schema_version": evidence.RESULTS_SCHEMA_VERSION,
        "runs": runs,
    }


def write_results(capture_dir: Path, document: dict[str, object]) -> bytes:
    payload = (json.dumps(document, indent=2, sort_keys=True) + "\n").encode()
    (capture_dir / evidence.RESULTS_NAME).write_bytes(payload)
    return payload


def results_manifest_line(payload: bytes) -> str:
    return f"{evidence.RESULTS_NAME}\tsha256={hashlib.sha256(payload).hexdigest()}"


class RealWindowSmokeEvidenceTests(unittest.TestCase):
    def test_expected_names_match_the_authoritative_75_capture_matrix(self) -> None:
        expected = known_expected_names()

        self.assertEqual(len(expected), 75)
        self.assertEqual(evidence.expected_capture_names(), expected)

    def test_fake_png_bytes_are_rejected_and_excluded_from_partial_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            name = "jellyflow-shader-compact-960x640-z0.5-interaction-initial.png"
            (capture_dir / name).write_bytes(b"png")

            with self.assertRaisesRegex(RuntimeError, rf"{name}: invalid PNG signature"):
                evidence.write_evidence_manifest(capture_dir)

            self.assertEqual(
                (capture_dir / evidence.MANIFEST_NAME).read_text(encoding="utf-8"), ""
            )

    def test_rgba8_capture_with_multiple_idat_chunks_is_supported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture-2x3-split-idat.png"
            payload = make_png(
                4,
                6,
                bit_depth=8,
                color_type=6,
                split_idat=True,
            )
            path.write_bytes(payload)

            capture = evidence.validate_capture(path)

            self.assertEqual((capture.width, capture.height, capture.scale), (4, 6, 2))
            self.assertEqual(capture.sha256, hashlib.sha256(payload).hexdigest())

    def test_crc_corruption_names_the_capture_and_excludes_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            name = "jellyflow-shader-compact-960x640-z1-interaction-selected.png"
            (capture_dir / name).write_bytes(
                make_png(960, 640, corrupt_ihdr_crc=True)
            )

            with self.assertRaisesRegex(RuntimeError, rf"{name}: IHDR CRC mismatch"):
                evidence.write_evidence_manifest(capture_dir)

            self.assertNotIn(
                name,
                (capture_dir / evidence.MANIFEST_NAME).read_text(encoding="utf-8"),
            )

    def test_truncated_png_names_the_capture_and_excludes_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            name = "jellyflow-workflow-compact-960x640-z2-interaction-final.png"
            (capture_dir / name).write_bytes(make_png(960, 640, truncate=5))

            with self.assertRaisesRegex(
                RuntimeError, rf"{name}: truncated PNG chunk header"
            ):
                evidence.write_evidence_manifest(capture_dir)

            self.assertNotIn(
                name,
                (capture_dir / evidence.MANIFEST_NAME).read_text(encoding="utf-8"),
            )

    def test_idat_scanline_length_must_match_ihdr(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            name = "jellyflow-workflow-compact-960x640-z1-interaction-selected.png"
            (capture_dir / name).write_bytes(make_png(960, 640, encoded_rows=639))

            with self.assertRaisesRegex(
                RuntimeError,
                rf"{name}: decoded scanline payload has .* expected",
            ):
                evidence.write_evidence_manifest(capture_dir)

            self.assertNotIn(
                name,
                (capture_dir / evidence.MANIFEST_NAME).read_text(encoding="utf-8"),
            )

    def test_width_and_height_must_use_the_same_integer_scale(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            name = "jellyflow-shader-compact-960x640-z2-interaction-final.png"
            (capture_dir / name).write_bytes(make_png(1920, 640))

            with self.assertRaisesRegex(
                RuntimeError, rf"{name}: width scale 2 and height scale 1 do not match"
            ):
                evidence.write_evidence_manifest(capture_dir)

    def test_non_integer_or_out_of_range_scale_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            non_integer = (
                "jellyflow-shader-compact-960x640-z1-interaction-initial.png"
            )
            too_large = "jellyflow-workflow-compact-960x640-z1-interaction-initial.png"
            (capture_dir / non_integer).write_bytes(make_png(961, 640))
            (capture_dir / too_large).write_bytes(make_png(4800, 3200))

            with self.assertRaises(RuntimeError) as context:
                evidence.write_evidence_manifest(capture_dir)

            message = str(context.exception)
            self.assertIn(f"{non_integer}: actual dimensions 961x640", message)
            self.assertIn(f"{too_large}: capture scale 5", message)

    def test_complete_matrix_writes_sorted_dimensions_and_hashes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            expected = known_expected_names()
            payloads = write_pngs(capture_dir, expected, scale=2)
            results_payload = write_results(capture_dir, valid_results_document())
            (capture_dir / "ignored.txt").write_text("ignored", encoding="utf-8")
            (capture_dir / evidence.MANIFEST_NAME).write_text(
                "stale evidence\n", encoding="utf-8"
            )

            manifest = evidence.write_evidence_manifest(capture_dir)

            expected_lines = [results_manifest_line(results_payload)]
            for name in sorted(expected):
                width, height = logical_size(name)
                expected_lines.append(
                    manifest_line(name, payloads[name], width * 2, height * 2)
                )
            self.assertEqual(
                manifest.read_text(encoding="utf-8"),
                "\n".join(expected_lines) + "\n",
            )

    def test_missing_capture_is_rejected_and_writes_verified_partial_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            names = known_expected_names()
            missing = "jellyflow-workflow-normal-1440x900-z1-interaction-final.png"
            names.remove(missing)
            payloads = write_pngs(capture_dir, names)
            results_payload = write_results(capture_dir, valid_results_document())

            with self.assertRaisesRegex(RuntimeError, f"missing=.*{missing}"):
                evidence.write_evidence_manifest(capture_dir)

            expected_lines = [results_manifest_line(results_payload)]
            for name in sorted(names):
                width, height = logical_size(name)
                expected_lines.append(manifest_line(name, payloads[name], width, height))
            self.assertEqual(
                (capture_dir / evidence.MANIFEST_NAME).read_text(encoding="utf-8"),
                "\n".join(expected_lines) + "\n",
            )

    def test_missing_and_unexpected_names_are_both_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            names = known_expected_names()
            missing = "jellyflow-shader-compact-960x640-z0.5-interaction-initial.png"
            names.remove(missing)
            write_pngs(capture_dir, names)
            unexpected = "jellyflow-shader-compact-960x640-z1-interaction-old-final.png"
            (capture_dir / unexpected).write_bytes(make_png(960, 640))

            with self.assertRaises(RuntimeError) as context:
                evidence.write_evidence_manifest(capture_dir)

            message = str(context.exception)
            self.assertIn(f"missing=['{missing}']", message)
            self.assertIn(f"unexpected=['{unexpected}']", message)
            self.assertIn(
                unexpected,
                (capture_dir / evidence.MANIFEST_NAME).read_text(encoding="utf-8"),
            )

    def test_matrix_rejects_mixed_backing_scales(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            names = known_expected_names()
            write_pngs(capture_dir, names)
            scaled = "jellyflow-shader-normal-1440x900-z1-interaction-final.png"
            (capture_dir / scaled).write_bytes(make_png(2880, 1800))

            with self.assertRaisesRegex(
                RuntimeError, r"capture matrix uses inconsistent scales=\[1, 2\]"
            ):
                evidence.write_evidence_manifest(capture_dir)

    def test_receipt_rejects_missing_and_duplicate_run_ids_by_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            results_path = Path(directory) / evidence.RESULTS_NAME
            missing_document = valid_results_document()
            missing_run = missing_document["runs"].pop()
            write_results(Path(directory), missing_document)

            with self.assertRaises(evidence.ResultsValidationError) as context:
                evidence.validate_results_receipt(results_path)

            message = str(context.exception)
            self.assertIn("run count is 8, expected exactly 9", message)
            self.assertIn(missing_run["run_id"], message)

            duplicate_document = valid_results_document()
            removed_run = duplicate_document["runs"][-1]
            duplicate_run = duplicate_document["runs"][0]
            duplicate_document["runs"][-1] = dict(duplicate_run)
            write_results(Path(directory), duplicate_document)

            with self.assertRaises(evidence.ResultsValidationError) as context:
                evidence.validate_results_receipt(results_path)

            message = str(context.exception)
            self.assertIn(
                f"duplicate run ids=['{duplicate_run['run_id']}']", message
            )
            self.assertIn(removed_run["run_id"], message)

    def test_receipt_rejects_tampered_exit_marker_and_marker_count(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            document = valid_results_document()
            run = document["runs"][0]
            run["exit_code"] = 7
            run["marker"] = run["marker"].replace("demo=shader", "demo=workflow")
            run["stdout"] += run["stdout"]
            write_results(capture_dir, document)

            with self.assertRaises(evidence.ResultsValidationError) as context:
                evidence.validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

            message = str(context.exception)
            self.assertIn(f"{run['run_id']}: exit_code is 7, expected 0", message)
            self.assertIn(
                f"{run['run_id']}: marker_count is 1, captured output proves 2",
                message,
            )
            self.assertIn(
                f"{run['run_id']}: stored marker does not match captured output",
                message,
            )

    def test_manifest_hash_binding_changes_with_a_valid_receipt_change(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            expected = known_expected_names()
            write_pngs(capture_dir, expected)
            document = valid_results_document()
            first_payload = write_results(capture_dir, document)

            manifest = evidence.write_evidence_manifest(capture_dir)
            first_lines = manifest.read_text(encoding="utf-8").splitlines()

            document["runs"][0]["stderr"] = "non-marker diagnostic\n"
            second_payload = write_results(capture_dir, document)
            manifest = evidence.write_evidence_manifest(capture_dir)
            second_lines = manifest.read_text(encoding="utf-8").splitlines()

            self.assertNotEqual(first_payload, second_payload)
            self.assertEqual(first_lines[0], results_manifest_line(first_payload))
            self.assertEqual(second_lines[0], results_manifest_line(second_payload))
            self.assertNotEqual(first_lines[0], second_lines[0])
            self.assertEqual(first_lines[1:], second_lines[1:])

    def test_capture_dir_output_uses_the_last_complete_matrix_run(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = root / "run-first"
            second = root / "run-second"
            output = root / "matrix-output.txt"
            output.write_text(
                f"unrelated=value\ncapture_dir={first}\ncapture_dir={second}\n",
                encoding="utf-8",
            )

            self.assertEqual(evidence.capture_dir_from_output(output), second)

            output.write_text("unrelated=value\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "missing capture_dir"):
                evidence.capture_dir_from_output(output)


if __name__ == "__main__":
    unittest.main()
