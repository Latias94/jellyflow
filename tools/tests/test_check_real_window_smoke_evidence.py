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
PORT_IDS = (
    "00000000-0000-0000-0000-000000008000",
    "00000000-0000-0000-0000-000000008001",
    "00000000-0000-0000-0000-000000008002",
    "00000000-0000-0000-0000-000000008003",
)
INITIAL_EDGE_ID = "00000000-0000-0000-0000-000000009000"
CURRENT_EDGE_ID = "00000000-0000-0000-0000-000000009001"
FOREIGN_NODE_ID = "ffffffff-ffff-ffff-ffff-ffffffffffff"


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


def final_marker(spec: Any, capture_dir: Path | None = None) -> str:
    logical_width, logical_height = evidence.RUN_WINDOW_SIZES[spec.bounds]
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
    fields = {field: "missing" for field in matrix.MARKER_FIELDS}
    fields.update(
        status="pass",
        demo=spec.demo,
        bounds=spec.bounds,
        scenario=spec.scenario,
        target_zoom=spec.zoom,
        reason="complete_frame",
        initial_nodes="2",
        initial_ports="4",
        initial_edges="1",
        initial_node_ids=",".join(NODE_IDS),
        initial_port_ids=",".join(PORT_IDS),
        initial_edge_ids=INITIAL_EDGE_ID,
        surface_width=str(logical_width - evidence.NATIVE_BLACKBOARD_LOGICAL_WIDTH),
        surface_height=str(logical_height),
        visible_expected_nodes="2",
        active_nodes="2",
        parked_nodes="0",
        mounts="2",
        restores="0",
        evictions="0",
        hard_retirements="0",
        protected_instances="0",
        protected_leases="0",
        protected_overflows="0",
        eligible_parked_nodes="0",
        strong_instances="2",
        frame_materialization_overflows="0",
        feedback_submissions="2",
        feedback_failures="0",
        layout_rejections="0",
        measurement_failures="0",
        geometry_layout_revision="7",
        current_geometry_nodes="2",
        current_geometry_ports="4",
        current_geometry_edges="1",
        current_geometry_node_ids=",".join(NODE_IDS),
        current_geometry_port_ids=",".join(PORT_IDS),
        current_geometry_edge_ids=CURRENT_EDGE_ID,
        geometry_fresh_nodes="2",
        geometry_rendered_ports="4",
        geometry_routed_edges="1",
        geometry_node_regions=(
            f"{NODE_IDS[0]}:40:40:240:180,{NODE_IDS[1]}:300:220:520:380"
        ),
        paint_image_width=str(capture.width if capture else logical_width),
        paint_image_height=str(capture.height if capture else logical_height),
        paint_canvas_width=str(
            capture.canvas_width
            if capture
            else logical_width - evidence.NATIVE_BLACKBOARD_LOGICAL_WIDTH
        ),
        paint_non_background_pixels=str(capture.non_background_pixels if capture else 400),
        paint_distinct_colors=str(capture.distinct_colors if capture else 4),
        paint_nodes=(
            f"{NODE_IDS[0]}:100/16/200,{NODE_IDS[1]}:120/16/220"
        ),
        input_evidence=spec.scenario,
        input_final_frame_validated="true",
        input_bounds=spec.bounds,
        input_bounds_verified="true",
        native_probe_failure="none",
        native_probe_phase="Complete",
        native_probe_wait_ticks="0",
        diagnostic="none",
    )
    if spec.scenario == "interaction":
        fields.update(
            input_dispatched_events="36",
            input_autopan_steering_events="0",
            input_target_zoom_events="36",
            input_target_zoom_captures="9",
            input_autopan_frames="3",
            input_selection_chrome_sides="4",
            input_selection_chrome_detached_run="0",
            input_connection_autopan_frames="3",
            **{key: "true" for key in matrix.INTERACTION_TRUE_FIELDS},
        )
    else:
        fields.update(
            input_lod_samples="3",
            input_lod_continuous="true",
            input_lod_max_jump_in_view="1.25",
        )
    return "JELLYFLOW_REAL_WINDOW_SMOKE " + " ".join(
        f"{key}={value}" for key, value in sorted(fields.items())
    )


def valid_results_document(capture_dir: Path | None = None) -> dict[str, object]:
    runs = []
    for spec in evidence.RUN_SPECS:
        marker = final_marker(spec, capture_dir)
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


def replace_run_marker_field(run: dict[str, object], key: str, value: str) -> None:
    fields = matrix.parse_marker_line(str(run["marker"]))
    fields[key] = value
    marker = "JELLYFLOW_REAL_WINDOW_SMOKE " + " ".join(
        f"{field}={field_value}" for field, field_value in sorted(fields.items())
    )
    run["marker"] = marker
    run["stdout"] = marker + "\n"


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
            write_pngs(capture_dir, expected, scale=2)
            results_payload = write_results(
                capture_dir, valid_results_document(capture_dir)
            )
            (capture_dir / "ignored.txt").write_text("ignored", encoding="utf-8")
            (capture_dir / evidence.MANIFEST_NAME).write_text(
                "stale evidence\n", encoding="utf-8"
            )

            manifest = evidence.write_evidence_manifest(capture_dir)

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
                evidence.write_evidence_manifest(capture_dir)

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

    def test_receipt_accepts_canonical_uuid_identity_schema(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            document = valid_results_document()
            write_results(capture_dir, document)

            result = evidence.validate_results_receipt(
                capture_dir / evidence.RESULTS_NAME
            )

            first_run_id = evidence.RUN_SPECS[0].run_id
            self.assertEqual(
                result.marker_fields[first_run_id]["initial_node_ids"],
                ",".join(NODE_IDS),
            )

    def test_receipt_rejects_malformed_uuid_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            document = valid_results_document()
            run = document["runs"][0]
            replace_run_marker_field(
                run,
                "initial_node_ids",
                f"not-a-uuid,{NODE_IDS[1]}",
            )
            write_results(capture_dir, document)

            with self.assertRaisesRegex(
                evidence.ResultsValidationError,
                "initial_node_ids identity is not a canonical UUID",
            ):
                evidence.validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

    def test_receipt_rejects_invalid_or_foreign_uuid_keyed_node_evidence(self) -> None:
        cases = (
            (
                "geometry_node_regions",
                f"not-a-uuid:40:40:240:180,{NODE_IDS[1]}:300:220:520:380",
                "geometry_node_regions node identity is not a canonical UUID",
            ),
            (
                "paint_nodes",
                f"not-a-uuid:100/16/200,{NODE_IDS[1]}:120/16/220",
                "paint_nodes node identity is not a canonical UUID",
            ),
            (
                "geometry_node_regions",
                f"{FOREIGN_NODE_ID}:40:40:240:180,"
                f"{NODE_IDS[1]}:300:220:520:380",
                "node-region identities differ from current geometry",
            ),
            (
                "paint_nodes",
                f"{FOREIGN_NODE_ID}:100/16/200,{NODE_IDS[1]}:120/16/220",
                "paint-node identities differ from current geometry",
            ),
        )
        for field, value, expected_error in cases:
            with self.subTest(
                field=field, value=value
            ), tempfile.TemporaryDirectory() as directory:
                capture_dir = Path(directory)
                document = valid_results_document()
                replace_run_marker_field(document["runs"][0], field, value)
                write_results(capture_dir, document)

                with self.assertRaisesRegex(
                    evidence.ResultsValidationError, expected_error
                ):
                    evidence.validate_results_receipt(
                        capture_dir / evidence.RESULTS_NAME
                    )

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
            document = valid_results_document(capture_dir)
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
            document = valid_results_document(capture_dir)
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

    def test_receipt_rejects_minimal_marker_and_false_or_zero_invariants(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            minimal = valid_results_document()
            run = minimal["runs"][0]
            marker = (
                "JELLYFLOW_REAL_WINDOW_SMOKE status=pass demo=shader bounds=compact "
                "scenario=interaction target_zoom=0.5"
            )
            run["marker"] = marker
            run["stdout"] = marker + "\n"
            write_results(capture_dir, minimal)

            with self.assertRaisesRegex(
                evidence.ResultsValidationError, "marker fields mismatch"
            ):
                evidence.validate_results_receipt(capture_dir / evidence.RESULTS_NAME)

            for key, value, message in (
                (
                    "paint_non_background_pixels",
                    "0",
                    "paint_non_background_pixels is 0",
                ),
                ("input_node_dragged", "false", "input_node_dragged is 'false'"),
            ):
                with self.subTest(key=key):
                    document = valid_results_document()
                    replace_run_marker_field(document["runs"][0], key, value)
                    write_results(capture_dir, document)
                    with self.assertRaisesRegex(
                        evidence.ResultsValidationError, message
                    ):
                        evidence.validate_results_receipt(
                            capture_dir / evidence.RESULTS_NAME
                        )

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
                evidence.write_evidence_manifest(capture_dir)

    def test_final_png_counts_are_bound_to_the_strict_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_dir = Path(directory)
            write_pngs(capture_dir, known_expected_names())
            document = valid_results_document(capture_dir)
            run = document["runs"][0]
            fields = matrix.parse_marker_line(str(run["marker"]))
            replace_run_marker_field(
                run,
                "paint_non_background_pixels",
                str(int(fields["paint_non_background_pixels"]) + 1),
            )
            write_results(capture_dir, document)

            with self.assertRaisesRegex(
                RuntimeError, "final PNG proves"
            ):
                evidence.write_evidence_manifest(capture_dir)

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
