from __future__ import annotations

import binascii
import hashlib
import importlib.util
import struct
import tempfile
import unittest
import zlib
from pathlib import Path


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
            (capture_dir / "ignored.txt").write_text("ignored", encoding="utf-8")
            (capture_dir / evidence.MANIFEST_NAME).write_text(
                "stale evidence\n", encoding="utf-8"
            )

            manifest = evidence.write_evidence_manifest(capture_dir)

            expected_lines = []
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

            with self.assertRaisesRegex(RuntimeError, f"missing=.*{missing}"):
                evidence.write_evidence_manifest(capture_dir)

            expected_lines = []
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


if __name__ == "__main__":
    unittest.main()
