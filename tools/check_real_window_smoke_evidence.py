#!/usr/bin/env python3
"""Validate and inventory the real-window smoke screenshots."""

from __future__ import annotations

import argparse
import binascii
import hashlib
import json
import os
import re
import struct
import sys
import tempfile
import zlib
from pathlib import Path
from typing import NamedTuple

try:
    from run_real_window_smoke_matrix import (
        RESULTS_NAME,
        RESULTS_SCHEMA_VERSION,
        RUN_SPECS,
        extract_final_marker_lines,
        marker_contract_errors,
    )
except ModuleNotFoundError:
    from tools.run_real_window_smoke_matrix import (
        RESULTS_NAME,
        RESULTS_SCHEMA_VERSION,
        RUN_SPECS,
        extract_final_marker_lines,
        marker_contract_errors,
    )


MANIFEST_NAME = "manifest.txt"
RESULTS_RECEIPT_KEYS = {
    "run_id",
    "config",
    "exit_code",
    "timed_out",
    "marker_count",
    "marker",
    "stdout",
    "stderr",
    "status",
    "errors",
}
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
CAPTURE_SIZE_PATTERN = re.compile(r"-(?P<width>[1-9]\d*)x(?P<height>[1-9]\d*)-")
MAX_CAPTURE_SCALE = 4
MAX_SCANLINE_BYTES = 128 * 1024 * 1024
INTERACTION_CAPTURE_STATES = (
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
LOD_CAPTURE_STATES = ("lod-0.86", "lod-0.85", "lod-0.84")
RUN_WINDOW_SIZES = {"compact": (960, 640), "normal": (1440, 900)}


class PngValidationError(RuntimeError):
    """Raised when a capture is not a structurally valid supported PNG."""


class ResultsValidationError(RuntimeError):
    """Raised when the matrix receipt does not prove the exact nine runs."""


class CaptureEvidence(NamedTuple):
    name: str
    width: int
    height: int
    scale: int
    sha256: str

    def manifest_line(self) -> str:
        return f"{self.name}\t{self.width}x{self.height}\tsha256={self.sha256}"


class ResultsEvidence(NamedTuple):
    name: str
    sha256: str

    def manifest_line(self) -> str:
        return f"{self.name}\tsha256={self.sha256}"


class PngHeader(NamedTuple):
    width: int
    height: int
    bit_depth: int
    color_type: int


def expected_capture_names() -> set[str]:
    names: set[str] = set()
    for spec in RUN_SPECS:
        width, height = RUN_WINDOW_SIZES[spec.bounds]
        states = (
            INTERACTION_CAPTURE_STATES
            if spec.scenario == "interaction"
            else LOD_CAPTURE_STATES
        )
        names.update(
            f"jellyflow-{spec.demo}-{spec.bounds}-{width}x{height}-z{spec.zoom}-"
            f"{spec.scenario}-{state}.png"
            for state in states
        )
    return names


def _logical_capture_size(name: str) -> tuple[int, int]:
    match = CAPTURE_SIZE_PATTERN.search(name)
    if match is None:
        raise PngValidationError("filename does not encode a logical WIDTHxHEIGHT")
    return int(match.group("width")), int(match.group("height"))


def _parse_ihdr(data: bytes) -> PngHeader:
    if len(data) != 13:
        raise PngValidationError(f"IHDR has length {len(data)}, expected 13")
    width, height, bit_depth, color_type, compression, filter_method, interlace = (
        struct.unpack(">IIBBBBB", data)
    )
    if width == 0 or height == 0:
        raise PngValidationError(f"IHDR dimensions must be positive, got {width}x{height}")
    if width > 0x7FFFFFFF or height > 0x7FFFFFFF:
        raise PngValidationError(f"IHDR dimensions exceed PNG limits: {width}x{height}")
    if compression != 0:
        raise PngValidationError(f"unsupported PNG compression method {compression}")
    if filter_method != 0:
        raise PngValidationError(f"unsupported PNG filter method {filter_method}")
    if interlace != 0:
        raise PngValidationError(f"unsupported PNG interlace method {interlace}")

    allowed_bit_depths = {
        0: {1, 2, 4, 8, 16},
        2: {8, 16},
        3: {1, 2, 4, 8},
        4: {8, 16},
        6: {8, 16},
    }
    if color_type not in allowed_bit_depths:
        raise PngValidationError(f"unsupported PNG color type {color_type}")
    if bit_depth not in allowed_bit_depths[color_type]:
        raise PngValidationError(
            f"unsupported bit depth {bit_depth} for PNG color type {color_type}"
        )
    return PngHeader(width, height, bit_depth, color_type)


def _decode_scanlines(header: PngHeader, compressed: bytes) -> None:
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[header.color_type]
    row_bytes = (header.width * header.bit_depth * channels + 7) // 8
    expected_bytes = header.height * (row_bytes + 1)
    if expected_bytes > MAX_SCANLINE_BYTES:
        raise PngValidationError(
            f"decoded scanlines exceed the {MAX_SCANLINE_BYTES}-byte evidence limit"
        )
    if not compressed:
        raise PngValidationError("PNG contains no IDAT payload")

    decoder = zlib.decompressobj()
    try:
        decoded = decoder.decompress(compressed, expected_bytes + 1)
        decoded += decoder.flush(max(1, expected_bytes + 1 - len(decoded)))
    except zlib.error as error:
        raise PngValidationError(f"IDAT zlib stream is invalid: {error}") from error
    if decoder.unconsumed_tail:
        raise PngValidationError("IDAT expands beyond the IHDR scanline dimensions")
    if not decoder.eof:
        raise PngValidationError("IDAT zlib stream is truncated")
    if decoder.unused_data:
        raise PngValidationError("IDAT contains data after the zlib stream")
    if not decoded:
        raise PngValidationError("IDAT decoded to an empty scanline payload")
    if len(decoded) != expected_bytes:
        raise PngValidationError(
            f"decoded scanline payload has {len(decoded)} bytes, expected {expected_bytes}"
        )

    stride = row_bytes + 1
    for row in range(header.height):
        filter_type = decoded[row * stride]
        if filter_type > 4:
            raise PngValidationError(
                f"scanline {row} has unsupported filter type {filter_type}"
            )


def _parse_png(contents: bytes) -> PngHeader:
    if not contents.startswith(PNG_SIGNATURE):
        raise PngValidationError("invalid PNG signature")

    offset = len(PNG_SIGNATURE)
    header: PngHeader | None = None
    idat_parts: list[bytes] = []
    saw_plte = False
    idat_ended = False
    saw_iend = False
    chunk_index = 0

    while offset < len(contents):
        if len(contents) - offset < 12:
            raise PngValidationError(f"truncated PNG chunk header at byte {offset}")
        length = struct.unpack_from(">I", contents, offset)[0]
        if length > 0x7FFFFFFF:
            raise PngValidationError(f"PNG chunk length exceeds the format limit at byte {offset}")
        chunk_type = contents[offset + 4 : offset + 8]
        chunk_end = offset + 12 + length
        if chunk_end > len(contents):
            label = chunk_type.decode("ascii", errors="replace")
            raise PngValidationError(f"truncated {label} chunk at byte {offset}")
        if len(chunk_type) != 4 or any(
            byte not in range(ord("A"), ord("Z") + 1)
            and byte not in range(ord("a"), ord("z") + 1)
            for byte in chunk_type
        ):
            raise PngValidationError(f"invalid PNG chunk type at byte {offset}")
        if chunk_type[2] not in range(ord("A"), ord("Z") + 1):
            raise PngValidationError(
                f"PNG chunk {chunk_type.decode('ascii')} sets the reserved type bit"
            )

        data_start = offset + 8
        data_end = data_start + length
        data = contents[data_start:data_end]
        recorded_crc = struct.unpack_from(">I", contents, data_end)[0]
        actual_crc = binascii.crc32(chunk_type)
        actual_crc = binascii.crc32(data, actual_crc) & 0xFFFFFFFF
        label = chunk_type.decode("ascii")
        if actual_crc != recorded_crc:
            raise PngValidationError(
                f"{label} CRC mismatch: recorded {recorded_crc:08x}, computed {actual_crc:08x}"
            )
        if chunk_index == 0 and chunk_type != b"IHDR":
            raise PngValidationError(f"first PNG chunk is {label}, expected IHDR")

        if chunk_type == b"IHDR":
            if header is not None:
                raise PngValidationError("PNG contains more than one IHDR chunk")
            header = _parse_ihdr(data)
        elif chunk_type == b"PLTE":
            if header is None:
                raise PngValidationError("PLTE appears before IHDR")
            if saw_plte:
                raise PngValidationError("PNG contains more than one PLTE chunk")
            if idat_parts:
                raise PngValidationError("PLTE appears after IDAT")
            if header.color_type in {0, 4}:
                raise PngValidationError(
                    f"PLTE is not allowed for PNG color type {header.color_type}"
                )
            if len(data) == 0 or len(data) % 3 != 0 or len(data) > 768:
                raise PngValidationError(f"PLTE has invalid length {len(data)}")
            if header.color_type == 3 and len(data) // 3 > 1 << header.bit_depth:
                raise PngValidationError("PLTE has more entries than the indexed bit depth permits")
            saw_plte = True
        elif chunk_type == b"IDAT":
            if header is None:
                raise PngValidationError("IDAT appears before IHDR")
            if idat_ended:
                raise PngValidationError("IDAT chunks are not consecutive")
            if header.color_type == 3 and not saw_plte:
                raise PngValidationError("indexed PNG is missing PLTE before IDAT")
            idat_parts.append(data)
        elif chunk_type == b"IEND":
            if length != 0:
                raise PngValidationError(f"IEND has length {length}, expected 0")
            if header is None:
                raise PngValidationError("IEND appears before IHDR")
            if not idat_parts:
                raise PngValidationError("PNG is missing IDAT")
            saw_iend = True
            offset = chunk_end
            if offset != len(contents):
                raise PngValidationError("PNG contains trailing data after IEND")
            break
        else:
            if chunk_type[0] in range(ord("A"), ord("Z") + 1):
                raise PngValidationError(f"unsupported critical PNG chunk {label}")
            if idat_parts:
                idat_ended = True

        offset = chunk_end
        chunk_index += 1

    if not saw_iend:
        raise PngValidationError("PNG is missing IEND")
    if header is None:
        raise PngValidationError("PNG is missing IHDR")
    _decode_scanlines(header, b"".join(idat_parts))
    return header


def validate_capture(path: Path) -> CaptureEvidence:
    logical_width, logical_height = _logical_capture_size(path.name)
    contents = path.read_bytes()
    header = _parse_png(contents)
    if header.width % logical_width != 0 or header.height % logical_height != 0:
        raise PngValidationError(
            f"actual dimensions {header.width}x{header.height} are not an integer scale of "
            f"logical dimensions {logical_width}x{logical_height}"
        )
    width_scale = header.width // logical_width
    height_scale = header.height // logical_height
    if width_scale != height_scale:
        raise PngValidationError(
            f"width scale {width_scale} and height scale {height_scale} do not match"
        )
    if not 1 <= width_scale <= MAX_CAPTURE_SCALE:
        raise PngValidationError(
            f"capture scale {width_scale} is outside the supported range 1..{MAX_CAPTURE_SCALE}"
        )
    return CaptureEvidence(
        name=path.name,
        width=header.width,
        height=header.height,
        scale=width_scale,
        sha256=hashlib.sha256(contents).hexdigest(),
    )


def _json_object_without_duplicate_keys(
    pairs: list[tuple[str, object]],
) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ResultsValidationError(f"JSON object contains duplicate key {key!r}")
        result[key] = value
    return result


def validate_results_receipt(path: Path) -> ResultsEvidence:
    contents = path.read_bytes()
    try:
        document = json.loads(
            contents.decode("utf-8"),
            object_pairs_hook=_json_object_without_duplicate_keys,
        )
    except UnicodeDecodeError as error:
        raise ResultsValidationError(f"results are not UTF-8: {error}") from error
    except json.JSONDecodeError as error:
        raise ResultsValidationError(f"results are not valid JSON: {error}") from error

    if not isinstance(document, dict):
        raise ResultsValidationError("results root must be a JSON object")
    root_keys = set(document)
    expected_root_keys = {"schema_version", "runs"}
    if root_keys != expected_root_keys:
        raise ResultsValidationError(
            "results root fields mismatch; "
            f"missing={sorted(expected_root_keys - root_keys)}; "
            f"unexpected={sorted(root_keys - expected_root_keys)}"
        )
    schema_version = document["schema_version"]
    if type(schema_version) is not int or schema_version != RESULTS_SCHEMA_VERSION:
        raise ResultsValidationError(
            f"schema_version is {schema_version!r}, expected {RESULTS_SCHEMA_VERSION}"
        )
    runs = document["runs"]
    if not isinstance(runs, list):
        raise ResultsValidationError("runs must be a JSON array")

    problems: list[str] = []
    if len(runs) != len(RUN_SPECS):
        problems.append(
            f"run count is {len(runs)}, expected exactly {len(RUN_SPECS)}"
        )

    entries_by_id: dict[str, list[dict[str, object]]] = {}
    for index, entry in enumerate(runs):
        if not isinstance(entry, dict):
            problems.append(f"runs[{index}] is not an object")
            continue
        run_id = entry.get("run_id")
        if not isinstance(run_id, str):
            problems.append(f"runs[{index}].run_id is not a string")
            continue
        entries_by_id.setdefault(run_id, []).append(entry)

    duplicate_ids = sorted(
        run_id for run_id, entries in entries_by_id.items() if len(entries) > 1
    )
    if duplicate_ids:
        problems.append(f"duplicate run ids={duplicate_ids}")
    expected_ids = {spec.run_id for spec in RUN_SPECS}
    actual_ids = set(entries_by_id)
    missing_ids = sorted(expected_ids - actual_ids)
    unexpected_ids = sorted(actual_ids - expected_ids)
    if missing_ids or unexpected_ids:
        problems.append(
            f"run id mismatch; missing={missing_ids}; unexpected={unexpected_ids}"
        )

    for spec in RUN_SPECS:
        entries = entries_by_id.get(spec.run_id, [])
        if len(entries) != 1:
            continue
        entry = entries[0]
        entry_keys = set(entry)
        if entry_keys != RESULTS_RECEIPT_KEYS:
            problems.append(
                f"{spec.run_id}: receipt fields mismatch; "
                f"missing={sorted(RESULTS_RECEIPT_KEYS - entry_keys)}; "
                f"unexpected={sorted(entry_keys - RESULTS_RECEIPT_KEYS)}"
            )

        if entry.get("config") != spec.config():
            problems.append(
                f"{spec.run_id}: config is {entry.get('config')!r}, "
                f"expected {spec.config()!r}"
            )

        exit_code_value = entry.get("exit_code")
        exit_code = exit_code_value if type(exit_code_value) is int else None
        if exit_code_value != 0 or type(exit_code_value) is not int:
            problems.append(
                f"{spec.run_id}: exit_code is {exit_code_value!r}, expected 0"
            )
        timed_out_value = entry.get("timed_out")
        timed_out = timed_out_value if isinstance(timed_out_value, bool) else False
        if timed_out_value is not False:
            problems.append(
                f"{spec.run_id}: timed_out is {timed_out_value!r}, expected false"
            )
        stdout_value = entry.get("stdout")
        stderr_value = entry.get("stderr")
        if not isinstance(stdout_value, str):
            problems.append(f"{spec.run_id}: stdout is not a string")
            stdout = ""
        else:
            stdout = stdout_value
        if not isinstance(stderr_value, str):
            problems.append(f"{spec.run_id}: stderr is not a string")
            stderr = ""
        else:
            stderr = stderr_value

        marker_lines = extract_final_marker_lines(stdout, stderr)
        marker_count = entry.get("marker_count")
        if type(marker_count) is not int or marker_count != len(marker_lines):
            problems.append(
                f"{spec.run_id}: marker_count is {marker_count!r}, "
                f"captured output proves {len(marker_lines)}"
            )
        expected_marker = marker_lines[0] if len(marker_lines) == 1 else None
        if entry.get("marker") != expected_marker:
            problems.append(
                f"{spec.run_id}: stored marker does not match captured output"
            )
        for error in marker_contract_errors(
            spec,
            exit_code=exit_code,
            timed_out=timed_out,
            marker_lines=marker_lines,
        ):
            problems.append(f"{spec.run_id}: {error}")
        if entry.get("status") != "pass":
            problems.append(
                f"{spec.run_id}: receipt status is {entry.get('status')!r}, expected 'pass'"
            )
        if entry.get("errors") != []:
            problems.append(
                f"{spec.run_id}: receipt errors are {entry.get('errors')!r}, expected []"
            )

    if problems:
        raise ResultsValidationError("; ".join(problems))
    return ResultsEvidence(path.name, hashlib.sha256(contents).hexdigest())


def _atomic_write_text(path: Path, contents: str) -> None:
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(contents)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def write_evidence_manifest(capture_dir: Path) -> Path:
    if not capture_dir.is_dir():
        raise RuntimeError(f"real-window smoke capture directory is missing: {capture_dir}")

    candidates = sorted(
        (
            path
            for path in capture_dir.iterdir()
            if path.is_file() and path.suffix == ".png"
        ),
        key=lambda path: path.name,
    )
    valid: list[CaptureEvidence] = []
    invalid: list[str] = []
    for path in candidates:
        try:
            valid.append(validate_capture(path))
        except (OSError, PngValidationError) as error:
            invalid.append(f"{path.name}: {error}")

    results_evidence: ResultsEvidence | None = None
    results_problem: str | None = None
    results_path = capture_dir / RESULTS_NAME
    try:
        results_evidence = validate_results_receipt(results_path)
    except (OSError, ResultsValidationError) as error:
        results_problem = f"{RESULTS_NAME}: {error}"

    manifest_path = capture_dir / MANIFEST_NAME
    manifest_lines = []
    if results_evidence is not None:
        manifest_lines.append(results_evidence.manifest_line())
    manifest_lines.extend(capture.manifest_line() for capture in valid)
    _atomic_write_text(
        manifest_path,
        "\n".join(manifest_lines) + ("\n" if manifest_lines else ""),
    )

    expected = expected_capture_names()
    valid_names = {capture.name for capture in valid}
    candidate_names = {path.name for path in candidates}
    missing = sorted(expected - valid_names)
    unexpected = sorted(candidate_names - expected)
    scales = sorted({capture.scale for capture in valid})
    problems: list[str] = []
    if results_problem is not None:
        problems.append(f"invalid matrix receipt={results_problem}")
    if invalid:
        problems.append(f"invalid PNGs={invalid}")
    if missing or unexpected:
        problems.append(f"matrix mismatch; missing={missing}; unexpected={unexpected}")
    if len(scales) > 1:
        problems.append(f"capture matrix uses inconsistent scales={scales}")
    if problems:
        raise RuntimeError(
            "real-window smoke capture validation failed; "
            + "; ".join(problems)
            + f"; partial manifest: {manifest_path}"
        )
    return manifest_path


def capture_dir_from_output(path: Path) -> Path:
    lines = path.read_text(encoding="utf-8").splitlines()
    capture_dirs = [
        line.removeprefix("capture_dir=")
        for line in lines
        if line.startswith("capture_dir=")
    ]
    if not capture_dirs or not capture_dirs[-1]:
        raise RuntimeError(f"capture directory output is missing capture_dir: {path}")
    return Path(capture_dirs[-1])


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Assert the exact real-window smoke screenshot matrix and write its manifest."
    )
    capture_source = parser.add_mutually_exclusive_group(required=True)
    capture_source.add_argument("--capture-dir", type=Path)
    capture_source.add_argument(
        "--capture-dir-from-output",
        type=Path,
        help="Read the last capture_dir entry written by the matrix orchestrator.",
    )
    args = parser.parse_args()

    try:
        capture_dir = (
            args.capture_dir
            if args.capture_dir is not None
            else capture_dir_from_output(args.capture_dir_from_output)
        )
        manifest = write_evidence_manifest(capture_dir)
    except (OSError, RuntimeError) as error:
        print(f"real-window smoke evidence validation failed: {error}", file=sys.stderr)
        return 1

    print(f"validated {len(expected_capture_names())} exact PNGs; manifest: {manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
