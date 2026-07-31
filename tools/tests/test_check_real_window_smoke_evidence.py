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

from tools import run_real_window_smoke_matrix as matrix


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "check_real_window_smoke_evidence.py"
SPEC = importlib.util.spec_from_file_location("check_real_window_smoke_evidence", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {SCRIPT_PATH}")
evidence = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evidence)


NODE_IDS = (
    "00000000-0000-0000-0000-000000007100",
    "00000000-0000-0000-0000-000000007200",
)
CURRENT_EDGE_ID = "00000000-0000-0000-0000-000000009001"


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
PNG_FIXTURE_CACHE: dict[tuple[int, int, int, str, bool, int], bytes] = {}
FIXTURE_PRODUCT_COLORS = {
    "shader": {
        "canvas": (0x0D, 0x12, 0x17, 0xFF),
        "nodes": ((0x1C, 0x24, 0x2B, 0xFF), (0x17, 0x26, 0x30, 0xFF)),
        "selection": (0x75, 0xB9, 0xD8, 0xFF),
        "focus": (0xA7, 0xD7, 0xEE, 0xFF),
    },
    "workflow": {
        "canvas": (0x10, 0x13, 0x18, 0xFF),
        "nodes": ((0x1C, 0x27, 0x33, 0xFF), (0x25, 0x27, 0x2B, 0xFF)),
        "selection": (0x6F, 0x94, 0xC9, 0xFF),
        "focus": (0xB8, 0xD1, 0xF1, 0xFF),
    },
}


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
    bit_depth: int = 8,
    color_type: int = 6,
    corrupt_ihdr_crc: bool = False,
    encoded_rows: int | None = None,
    split_idat: bool = False,
    truncate: int = 0,
    canvas_width: int | None = None,
    scene: bool = True,
    selected: bool = False,
    variant: int = 0,
    filter_type: int = 0,
    product: str = "shader",
    selection_color: tuple[int, int, int, int] | None = None,
    edge_color: tuple[int, int, int, int] = (0x59, 0xC2, 0xA6, 0xFF),
) -> bytes:
    ihdr = struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, 0)
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color_type]
    row_count = height if encoded_rows is None else encoded_rows
    row_bytes = (width * bit_depth * channels + 7) // 8
    bytes_per_pixel = max(1, (bit_depth * channels + 7) // 8)
    previous = bytes(row_bytes)
    compressor = zlib.compressobj()
    compressed_parts: list[bytes] = []
    for y in range(row_count):
        if bit_depth == 8 and color_type == 6 and scene:
            active_canvas_width = width if canvas_width is None else canvas_width
            raw = scene_row(
                width,
                height,
                active_canvas_width,
                y,
                selected=selected,
                variant=variant,
                product=product,
                selection_color=selection_color,
                edge_color=edge_color,
            )
        elif bit_depth == 8 and color_type == 6:
            raw = bytes((0, 0, 0, 255)) * width
        else:
            raw = bytes(row_bytes)
        encoded = encode_png_filter(raw, previous, bytes_per_pixel, filter_type)
        compressed_parts.append(compressor.compress(bytes((filter_type,)) + encoded))
        previous = raw
    compressed_parts.append(compressor.flush())
    compressed = b"".join(compressed_parts)
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


def scene_row(
    width: int,
    height: int,
    canvas_width: int,
    y: int,
    *,
    selected: bool,
    variant: int,
    product: str,
    selection_color: tuple[int, int, int, int] | None,
    edge_color: tuple[int, int, int, int],
) -> bytes:
    palette = FIXTURE_PRODUCT_COLORS[product]
    background = bytes(palette["canvas"])
    row = bytearray(background * width)
    row[canvas_width * 4 :] = bytes((0x18, 0x1C, 0x22, 0xFF)) * (width - canvas_width)
    grid = bytes((0x1B, 0x20, 0x28, 0xFF))
    if y % 32 == 0:
        row[: canvas_width * 4] = grid * canvas_width
    else:
        for x in range(0, canvas_width, 32):
            row[x * 4 : x * 4 + 4] = grid

    def fill(left: int, right: int, color: tuple[int, int, int, int]) -> None:
        clipped_left = max(0, min(canvas_width, left))
        clipped_right = max(clipped_left, min(canvas_width, right))
        row[clipped_left * 4 : clipped_right * 4] = bytes(color) * (
            clipped_right - clipped_left
        )

    if 80 <= y < min(height, 220):
        fill(90, 310, palette["nodes"][0])
        if y % 18 < 3:
            fill(115, 260, (0xD2, 0xD8, 0xE1, 0xFF))
    if 270 <= y < min(height, 430):
        fill(360, 590, palette["nodes"][1])
    if 245 <= y < 250:
        fill(300, 420, edge_color)
    selection = palette["selection"] if selection_color is None else selection_color
    if selected and (y in range(78, 82) or y in range(218, 222)):
        fill(88, 312, selection)
    if selected and 82 <= y < 218:
        fill(88, 92, selection)
        fill(308, 312, selection)
    variant_y = 12 + variant % max(1, min(height - 12, 200))
    if variant_y <= y < variant_y + 4:
        color = (0x60 + variant % 64, 0x80, 0xB0, 0xFF)
        fill(12, 20, color)
    return bytes(row)


def encode_png_filter(raw: bytes, previous: bytes, bpp: int, filter_type: int) -> bytes:
    if filter_type == 0:
        return raw
    encoded = bytearray(len(raw))
    for index, value in enumerate(raw):
        left = raw[index - bpp] if index >= bpp else 0
        above = previous[index]
        upper_left = previous[index - bpp] if index >= bpp else 0
        if filter_type == 1:
            predictor = left
        elif filter_type == 2:
            predictor = above
        elif filter_type == 3:
            predictor = (left + above) // 2
        elif filter_type == 4:
            predictor = evidence._paeth_predictor(left, above, upper_left)
        else:
            raise ValueError(filter_type)
        encoded[index] = (value - predictor) & 0xFF
    return bytes(encoded)


def logical_size(name: str) -> tuple[int, int]:
    match = evidence.CAPTURE_SIZE_PATTERN.search(name)
    if match is None:
        raise AssertionError(f"fixture name has no size: {name}")
    return int(match.group("width")), int(match.group("height"))


def write_pngs(capture_dir: Path, names: set[str], *, scale: int = 1) -> dict[str, bytes]:
    payloads: dict[str, bytes] = {}
    for variant, name in enumerate(sorted(names), start=1):
        width, height = logical_size(name)
        actual_width = width * scale
        actual_height = height * scale
        canvas_width = (width - evidence.NATIVE_BLACKBOARD_LOGICAL_WIDTH) * scale
        selected = name.endswith("-interaction-selected.png")
        product = name.split("-", maxsplit=2)[1]
        cache_key = (
            actual_width,
            actual_height,
            canvas_width,
            product,
            selected,
            variant,
        )
        payload = PNG_FIXTURE_CACHE.get(cache_key)
        if payload is None:
            payload = make_png(
                actual_width,
                actual_height,
                canvas_width=canvas_width,
                selected=selected,
                variant=variant,
                product=product,
            )
            PNG_FIXTURE_CACHE[cache_key] = payload
        (capture_dir / name).write_bytes(payload)
        payloads[name] = payload
    return payloads


def canonical_receipt(spec: Any, capture_dir: Path | None = None) -> dict[str, object]:
    window_width, window_height = evidence.RUN_WINDOW_SIZES[spec.bounds]
    final_state = "final" if spec.scenario == "interaction" else "lod-0.84"
    capture_path = (
        capture_dir / evidence._capture_name(spec, final_state)
        if capture_dir is not None
        else None
    )
    capture = (
        evidence.validate_capture(capture_path)
        if capture_path is not None and capture_path.is_file()
        else None
    )
    return {
        "schema_version": 1,
        "status": "pass",
        "run": spec.receipt_run(),
        "failure": None,
        "evidence": {
            "paint": {
                "image_width": capture.width if capture else window_width,
                "image_height": capture.height if capture else window_height,
                "canvas_width": (
                    capture.canvas_width
                    if capture
                    else window_width - evidence.NATIVE_BLACKBOARD_LOGICAL_WIDTH
                ),
                "non_background_pixels": (
                    capture.non_background_pixels if capture else 400
                ),
                "distinct_colors": capture.distinct_colors if capture else 4,
                "painted_node_ids": list(NODE_IDS),
                "painted_edge_ids": [CURRENT_EDGE_ID],
            }
        },
        "presented_frames": [],
    }


def receipt_line(receipt: dict[str, object]) -> str:
    return matrix.RECEIPT_PREFIX + json.dumps(receipt, separators=(",", ":"))


def valid_results_document(capture_dir: Path | None = None) -> dict[str, object]:
    runs = []
    for spec in evidence.RUN_SPECS:
        receipt = canonical_receipt(spec, capture_dir)
        line = receipt_line(receipt)
        runs.append(
            {
                "run_id": spec.run_id,
                "config": spec.config(),
                "exit_code": 0,
                "timed_out": False,
                "receipt_count": 1,
                "receipt": receipt,
                "stdout": line + "\n",
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


def refresh_run_output(run: dict[str, object]) -> None:
    receipt = run["receipt"]
    if not isinstance(receipt, dict):
        raise TypeError("fixture receipt must be an object")
    run["stdout"] = receipt_line(receipt) + "\n"


VALIDATOR_CALLS = 0


def fake_validate_receipt(
    payload: str, *, receipt_validator: Path, environment: object
) -> dict[str, object]:
    global VALIDATOR_CALLS
    del receipt_validator, environment
    VALIDATOR_CALLS += 1
    try:
        value = json.loads(
            payload,
            object_pairs_hook=matrix.reject_duplicate_json_object_keys,
        )
    except (json.JSONDecodeError, ValueError) as error:
        raise matrix.ReceiptTransportError(
            "malformed_receipt", f"fixture validator rejected malformed JSON: {error}"
        ) from error
    if not isinstance(value, dict):
        raise matrix.ReceiptTransportError(
            "validator_failure", "fixture validator requires an object"
        )
    return value


evidence.validate_receipt = fake_validate_receipt


def validate_results_receipt(path: Path) -> evidence.ResultsEvidence:
    return evidence.validate_results_receipt(
        path,
        receipt_validator=Path("fixture-validator"),
        environment={},
    )


def write_evidence_manifest(capture_dir: Path) -> Path:
    return evidence.write_evidence_manifest(
        capture_dir,
        receipt_validator=Path("fixture-validator"),
        environment={},
    )


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
                write_evidence_manifest(capture_dir)

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

            header, scanlines = evidence._parse_png(payload)

            self.assertEqual((header.width, header.height), (4, 6))
            self.assertEqual(len(scanlines), 4 * 6 * 4)

    def test_all_png_filters_decode_to_the_same_rgba_pixels(self) -> None:
        decoded = []
        for filter_type in range(5):
            payload = make_png(
                32,
                24,
                canvas_width=32,
                variant=7,
                selected=True,
                filter_type=filter_type,
            )
            header, scanlines = evidence._parse_png(payload)
            self.assertEqual((header.width, header.height), (32, 24))
            decoded.append(scanlines)

        self.assertTrue(all(scanlines == decoded[0] for scanlines in decoded[1:]))

    def test_opaque_black_capture_is_rejected_as_blank_visual_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = (
                Path(directory)
                / "jellyflow-shader-compact-960x640-z1-interaction-initial.png"
            )
            path.write_bytes(make_png(960, 640, scene=False))

            with self.assertRaisesRegex(
                evidence.PngValidationError, "exact background pixels"
            ):
                evidence.validate_capture(path)

    def test_selected_capture_requires_selection_color_pixels(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = (
                Path(directory)
                / "jellyflow-shader-compact-960x640-z1-interaction-selected.png"
            )
            path.write_bytes(make_png(960, 640, canvas_width=680, selected=False))

            with self.assertRaisesRegex(
                evidence.PngValidationError, "selection-color pixels"
            ):
                evidence.validate_capture(path)

    def test_capture_palette_is_bound_to_each_product_filename(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            for product in ("shader", "workflow"):
                with self.subTest(product=product, state="initial"):
                    initial = (
                        capture_dir
                        / f"jellyflow-{product}-compact-960x640-z1-interaction-initial.png"
                    )
                    initial.write_bytes(
                        make_png(960, 640, canvas_width=680, product=product)
                    )
                    initial_evidence = evidence.validate_capture(initial)
                    self.assertGreaterEqual(
                        initial_evidence.background_pixels,
                        evidence.MIN_CANVAS_BACKGROUND_PIXELS,
                    )
                    self.assertGreaterEqual(
                        initial_evidence.node_surface_pixels,
                        evidence.MIN_NODE_SURFACE_PIXELS,
                    )

                for token in ("selection", "focus"):
                    with self.subTest(product=product, state=token):
                        selected = (
                            capture_dir
                            / f"jellyflow-{product}-compact-960x640-z1-"
                            "interaction-selected.png"
                        )
                        selected.write_bytes(
                            make_png(
                                960,
                                640,
                                canvas_width=680,
                                selected=True,
                                product=product,
                                selection_color=FIXTURE_PRODUCT_COLORS[product][token],
                            )
                        )
                        selected_evidence = evidence.validate_capture(selected)
                        self.assertGreaterEqual(
                            selected_evidence.selection_pixels,
                            evidence.MIN_SELECTION_PIXELS,
                        )

    def test_selected_capture_rejects_another_products_selection_palette(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = (
                Path(directory)
                / "jellyflow-workflow-compact-960x640-z1-interaction-selected.png"
            )
            path.write_bytes(
                make_png(
                    960,
                    640,
                    canvas_width=680,
                    selected=True,
                    product="workflow",
                    selection_color=FIXTURE_PRODUCT_COLORS["shader"]["selection"],
                )
            )

            with self.assertRaisesRegex(
                evidence.PngValidationError, "selection-color pixels"
            ):
                evidence.validate_capture(path)

    def test_selected_capture_does_not_mistake_a_near_color_edge_for_chrome(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = (
                Path(directory)
                / "jellyflow-workflow-compact-960x640-z1-interaction-selected.png"
            )
            path.write_bytes(
                make_png(
                    960,
                    640,
                    canvas_width=680,
                    selected=False,
                    product="workflow",
                    edge_color=(0x83, 0x94, 0xC9, 0xFF),
                )
            )

            with self.assertRaisesRegex(
                evidence.PngValidationError, "selection-color pixels"
            ):
                evidence.validate_capture(path)

    def test_crc_corruption_names_the_capture_and_excludes_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            name = "jellyflow-shader-compact-960x640-z1-interaction-selected.png"
            (capture_dir / name).write_bytes(
                make_png(960, 640, corrupt_ihdr_crc=True)
            )

            with self.assertRaisesRegex(RuntimeError, rf"{name}: IHDR CRC mismatch"):
                write_evidence_manifest(capture_dir)

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
                write_evidence_manifest(capture_dir)

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
                write_evidence_manifest(capture_dir)

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
                write_evidence_manifest(capture_dir)

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
                write_evidence_manifest(capture_dir)

            message = str(context.exception)
            self.assertIn(f"{non_integer}: actual dimensions 961x640", message)
            self.assertIn(f"{too_large}: capture scale 5", message)

    def test_complete_matrix_writes_sorted_dimensions_and_hashes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            expected = known_expected_names()
            write_pngs(capture_dir, expected, scale=2)
            results_payload = write_results(
                capture_dir, valid_results_document(capture_dir)
            )
            (capture_dir / "ignored.txt").write_text("ignored", encoding="utf-8")
            (capture_dir / evidence.MANIFEST_NAME).write_text(
                "stale evidence\n", encoding="utf-8"
            )

            manifest = write_evidence_manifest(capture_dir)

            expected_lines = [results_manifest_line(results_payload)]
            for name in sorted(expected):
                expected_lines.append(
                    evidence.validate_capture(capture_dir / name).manifest_line()
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
            write_pngs(capture_dir, names)
            results_payload = write_results(
                capture_dir, valid_results_document(capture_dir)
            )

            with self.assertRaisesRegex(RuntimeError, f"missing=.*{missing}"):
                write_evidence_manifest(capture_dir)

            expected_lines = [results_manifest_line(results_payload)]
            for name in sorted(names):
                expected_lines.append(
                    evidence.validate_capture(capture_dir / name).manifest_line()
                )
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
                write_evidence_manifest(capture_dir)

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
                write_evidence_manifest(capture_dir)

    def test_receipt_accepts_canonical_json_from_the_runner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            document = valid_results_document()
            write_results(capture_dir, document)

            result = validate_results_receipt(
                capture_dir / evidence.RESULTS_NAME
            )

            first_run_id = evidence.RUN_SPECS[0].run_id
            self.assertEqual(
                result.receipts[first_run_id]["run"],
                evidence.RUN_SPECS[0].receipt_run(),
            )

    def test_checker_routes_every_receipt_through_the_contract_boundary(self) -> None:
        global VALIDATOR_CALLS
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            document = valid_results_document()
            write_results(capture_dir, document)
            calls_before = VALIDATOR_CALLS

            validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

            self.assertEqual(VALIDATOR_CALLS - calls_before, len(evidence.RUN_SPECS))

    def test_receipt_rejects_missing_and_duplicate_run_ids_by_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            results_path = Path(directory) / evidence.RESULTS_NAME
            missing_document = valid_results_document()
            missing_run = missing_document["runs"].pop()
            write_results(Path(directory), missing_document)

            with self.assertRaises(evidence.ResultsValidationError) as context:
                validate_results_receipt(results_path)

            message = str(context.exception)
            self.assertIn("run count is 8, expected exactly 9", message)
            self.assertIn(missing_run["run_id"], message)

            duplicate_document = valid_results_document()
            removed_run = duplicate_document["runs"][-1]
            duplicate_run = duplicate_document["runs"][0]
            duplicate_document["runs"][-1] = dict(duplicate_run)
            write_results(Path(directory), duplicate_document)

            with self.assertRaises(evidence.ResultsValidationError) as context:
                validate_results_receipt(results_path)

            message = str(context.exception)
            self.assertIn(
                f"duplicate run ids=['{duplicate_run['run_id']}']", message
            )
            self.assertIn(removed_run["run_id"], message)

    def test_receipt_rejects_failed_transport_envelopes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            cases = (
                ({"exit_code": 7}, "exit_code is 7, expected 0"),
                ({"timed_out": True}, "timed_out is True, expected false"),
                ({"status": "fail"}, "receipt status is 'fail', expected 'pass'"),
                (
                    {"errors": ["transport.timeout: fixture"]},
                    "receipt errors are ['transport.timeout: fixture'], expected []",
                ),
                ({"receipt": None}, "receipt is not a canonical JSON object"),
            )
            for changes, expected in cases:
                with self.subTest(changes=changes):
                    document = valid_results_document(capture_dir)
                    document["runs"][0].update(changes)
                    write_results(capture_dir, document)
                    with self.assertRaises(
                        evidence.ResultsValidationError
                    ) as context:
                        validate_results_receipt(
                            capture_dir / evidence.RESULTS_NAME
                        )
                    self.assertIn(expected, str(context.exception))

    def test_receipt_count_and_stored_payload_are_bound_to_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            document = valid_results_document(capture_dir)
            run = document["runs"][0]
            run["stdout"] += run["stdout"]
            write_results(capture_dir, document)

            with self.assertRaisesRegex(
                evidence.ResultsValidationError,
                "receipt_count is 1, captured output proves 2",
            ):
                validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

            document = valid_results_document(capture_dir)
            run = document["runs"][0]
            run["receipt"]["status"] = "fail"
            write_results(capture_dir, document)
            with self.assertRaisesRegex(
                evidence.ResultsValidationError,
                "stored receipt does not match Rust-canonical output",
            ):
                validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

    def test_receipt_rejects_malformed_emitted_json_and_run_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            document = valid_results_document()
            document["runs"][0]["stdout"] = matrix.RECEIPT_PREFIX + "{not-json\n"
            write_results(capture_dir, document)
            with self.assertRaisesRegex(
                evidence.ResultsValidationError, "Rust receipt validation failed"
            ):
                validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

            document = valid_results_document()
            run = document["runs"][0]
            run["receipt"]["run"]["demo"] = "workflow"
            refresh_run_output(run)
            write_results(capture_dir, document)
            with self.assertRaisesRegex(
                evidence.ResultsValidationError, "canonical receipt run is"
            ):
                validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

    def test_manifest_hash_binding_changes_with_a_valid_receipt_change(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            expected = known_expected_names()
            write_pngs(capture_dir, expected)
            document = valid_results_document(capture_dir)
            first_payload = write_results(capture_dir, document)

            manifest = write_evidence_manifest(capture_dir)
            first_lines = manifest.read_text(encoding="utf-8").splitlines()

            document["runs"][0]["stderr"] = "non-receipt diagnostic\n"
            second_payload = write_results(capture_dir, document)
            manifest = write_evidence_manifest(capture_dir)
            second_lines = manifest.read_text(encoding="utf-8").splitlines()

            self.assertNotEqual(first_payload, second_payload)
            self.assertEqual(first_lines[0], results_manifest_line(first_payload))
            self.assertEqual(second_lines[0], results_manifest_line(second_payload))
            self.assertNotEqual(first_lines[0], second_lines[0])
            self.assertEqual(first_lines[1:], second_lines[1:])

    def test_matrix_rejects_identical_preview_and_committed_pixels(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            names = known_expected_names()
            write_pngs(capture_dir, names)
            preview = (
                capture_dir
                / "jellyflow-shader-compact-960x640-z1-interaction-connection-preview.png"
            )
            committed = (
                capture_dir
                / "jellyflow-shader-compact-960x640-z1-interaction-connection-committed.png"
            )
            committed.write_bytes(preview.read_bytes())
            write_results(capture_dir, valid_results_document(capture_dir))

            with self.assertRaisesRegex(
                RuntimeError,
                "connection-preview and connection-committed decode to identical pixels",
            ):
                write_evidence_manifest(capture_dir)

    def test_final_png_counts_are_bound_to_canonical_receipt_paint(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            write_pngs(capture_dir, known_expected_names())
            document = valid_results_document(capture_dir)
            run = document["runs"][0]
            paint = run["receipt"]["evidence"]["paint"]
            paint["non_background_pixels"] += 1
            refresh_run_output(run)
            write_results(capture_dir, document)

            with self.assertRaisesRegex(
                RuntimeError, "final PNG proves"
            ):
                write_evidence_manifest(capture_dir)

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
