#!/usr/bin/env python3
"""Build a temporary external Jellyflow consumer outside the Fret workspace."""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path


def write_file(path: Path, contents: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Create a temporary Cargo project outside this workspace and check that it can "
            "consume jellyflow-core and jellyflow-runtime without fret-node or fret-core."
        )
    )
    parser.add_argument(
        "--keep",
        action="store_true",
        help="keep the temporary smoke project for inspection",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]

    temp_context = tempfile.TemporaryDirectory(prefix="jellyflow_external_smoke_")
    temp_dir = Path(temp_context.name)
    if args.keep:
        temp_context.cleanup()
        temp_dir = Path(tempfile.mkdtemp(prefix="jellyflow_external_smoke_"))

    try:
        write_file(
            temp_dir / "Cargo.toml",
            textwrap.dedent(
                f"""
                [package]
                name = "jellyflow-external-smoke"
                version = "0.0.0"
                edition = "2024"
                rust-version = "1.92"
                publish = false

                [dependencies]
                jellyflow-core = {{ path = "{(repo_root / "crates/jellyflow-core").as_posix()}" }}
                jellyflow-runtime = {{ path = "{(repo_root / "crates/jellyflow-runtime").as_posix()}" }}
                """
            ).strip()
            + "\n",
        )
        write_file(
            temp_dir / "src/main.rs",
            textwrap.dedent(
                """
                use jellyflow_core::{
                    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphId, GraphOp,
                    GraphTransaction, Node, NodeGraphModifierKey, NodeGraphModifiers, NodeId,
                    NodeKindKey,
                };
                use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
                use jellyflow_runtime::runtime::fit_view::{
                    compute_fit_view_target_for_canvas_rect, FitViewComputeOptions,
                };
                use jellyflow_runtime::runtime::geometry::{
                    bezier_edge_path, edge_path_contains_point, edge_position, BezierEdgeOptions,
                    EdgeEndpointInput, EdgeHitTestOptions, HandleBounds, HandlePosition,
                };
                use jellyflow_runtime::NodeGraphStore;

                fn make_node(kind: &str, x: f32, y: f32) -> Node {
                    Node {
                        kind: NodeKindKey::new(kind),
                        kind_version: 1,
                        pos: CanvasPoint { x, y },
                        selectable: None,
                        draggable: None,
                        connectable: None,
                        deletable: None,
                        parent: None,
                        extent: None,
                        expand_parent: None,
                        size: Some(CanvasSize {
                            width: 160.0,
                            height: 80.0,
                        }),
                        hidden: false,
                        collapsed: false,
                        ports: Vec::new(),
                        data: Default::default(),
                    }
                }

                fn main() {
                    let node_id = NodeId::from_u128(2);
                    let node = make_node("demo.source", 10.0, 20.0);
                    let mut graph = Graph::new(GraphId::from_u128(1));

                    let mut add = GraphTransaction::new().with_label("add demo node");
                    add.push(GraphOp::AddNode { id: node_id, node });
                    add.apply_to(&mut graph).expect("transaction applies");
                    assert!(graph.nodes.contains_key(&node_id));

                    let mut store = NodeGraphStore::new(
                        graph,
                        NodeGraphViewState::default(),
                        NodeGraphEditorConfig::default(),
                    );
                    let mut move_node = GraphTransaction::new().with_label("move demo node");
                    move_node.push(GraphOp::SetNodePos {
                        id: node_id,
                        from: CanvasPoint { x: 10.0, y: 20.0 },
                        to: CanvasPoint { x: 32.0, y: 48.0 },
                    });
                    let outcome = store
                        .dispatch_transaction(&move_node)
                        .expect("store dispatch succeeds");
                    assert_eq!(outcome.committed().ops.len(), 1);
                    assert_eq!(store.graph().nodes[&node_id].pos, CanvasPoint { x: 32.0, y: 48.0 });

                    let (pan, zoom) = compute_fit_view_target_for_canvas_rect(
                        CanvasRect {
                            origin: CanvasPoint { x: 32.0, y: 48.0 },
                            size: CanvasSize {
                                width: 160.0,
                                height: 80.0,
                            },
                        },
                        FitViewComputeOptions {
                            viewport_width_px: 800.0,
                            viewport_height_px: 600.0,
                            node_origin: (0.0, 0.0),
                            padding: 0.1,
                            margin_px_fallback: 24.0,
                            min_zoom: 0.1,
                            max_zoom: 4.0,
                        },
                    )
                    .expect("fit-view target");
                    assert!(pan.x.is_finite() && pan.y.is_finite());
                    assert!(zoom.is_finite() && zoom > 0.0);

                    let endpoints = edge_position(
                        EdgeEndpointInput {
                            node_rect: CanvasRect {
                                origin: CanvasPoint { x: 32.0, y: 48.0 },
                                size: CanvasSize {
                                    width: 160.0,
                                    height: 80.0,
                                },
                            },
                            handle: Some(HandleBounds {
                                rect: CanvasRect {
                                    origin: CanvasPoint { x: 152.0, y: 32.0 },
                                    size: CanvasSize {
                                        width: 8.0,
                                        height: 16.0,
                                    },
                                },
                                position: HandlePosition::Right,
                            }),
                            fallback_position: HandlePosition::Right,
                        },
                        EdgeEndpointInput {
                            node_rect: CanvasRect {
                                origin: CanvasPoint { x: 320.0, y: 96.0 },
                                size: CanvasSize {
                                    width: 160.0,
                                    height: 80.0,
                                },
                            },
                            handle: Some(HandleBounds {
                                rect: CanvasRect {
                                    origin: CanvasPoint { x: 0.0, y: 32.0 },
                                    size: CanvasSize {
                                        width: 8.0,
                                        height: 16.0,
                                    },
                                },
                                position: HandlePosition::Left,
                            }),
                            fallback_position: HandlePosition::Left,
                        },
                    )
                    .expect("edge endpoints");
                    let edge_path = bezier_edge_path(
                        endpoints.source,
                        endpoints.target,
                        BezierEdgeOptions::default(),
                    )
                    .expect("edge path");
                    assert!(edge_path_contains_point(
                        &edge_path,
                        edge_path.label.point,
                        EdgeHitTestOptions::default(),
                    ));

                    assert!(NodeGraphModifierKey::CtrlOrMeta.is_pressed(NodeGraphModifiers {
                        ctrl: true,
                        ..NodeGraphModifiers::default()
                    }));
                }
                """
            ).strip()
            + "\n",
        )

        print(f"external smoke project: {temp_dir}", flush=True)
        subprocess.run(
            ["cargo", "check", "--manifest-path", str(temp_dir / "Cargo.toml")],
            cwd=repo_root,
            check=True,
        )
        tree = subprocess.run(
            [
                "cargo",
                "tree",
                "--manifest-path",
                str(temp_dir / "Cargo.toml"),
                "--prefix",
                "none",
            ],
            cwd=repo_root,
            check=True,
            capture_output=True,
            text=True,
        )
        forbidden = []
        for line in tree.stdout.splitlines():
            package_name = line.strip().split(" ", 1)[0]
            if package_name == "fret" or package_name.startswith("fret-"):
                forbidden.append(line.strip())
        if forbidden:
            print(
                "external Jellyflow smoke pulled Fret packages:\n"
                + "\n".join(forbidden),
                file=sys.stderr,
            )
            return 1
        print("external cargo tree contains no fret or fret-* packages")
        return 0
    finally:
        if args.keep:
            print(f"kept external smoke project: {temp_dir}")
        else:
            temp_context.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
