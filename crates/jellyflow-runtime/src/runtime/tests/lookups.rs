use super::fixtures::make_graph;

use crate::runtime::lookups::{ConnectionSide, NodeGraphLookups};

#[test]
fn lookups_rebuild_populates_connection_lookup() {
    let (g, a, b, out_port, in_port, eid) = make_graph();

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    assert!(lookups.node_lookup.contains_key(&a));
    assert!(lookups.node_lookup.contains_key(&b));
    assert_eq!(lookups.node_lookup.get(&a).unwrap().ports, vec![out_port]);
    assert_eq!(lookups.node_lookup.get(&b).unwrap().ports, vec![in_port]);

    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().from, out_port);
    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().to, in_port);

    let a_out = lookups
        .connections_for_port(a, ConnectionSide::Source, out_port)
        .expect("connections");
    assert_eq!(a_out.get(&eid).unwrap().target_node, b);

    let b_all = lookups.connections_for_node(b).expect("connections");
    assert!(b_all.contains_key(&eid));
}

#[test]
fn lookups_connections_for_node_side_filters_by_direction() {
    let (g, a, b, out_port, in_port, eid) = make_graph();

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let a_source = lookups
        .connections_for_node_side(a, ConnectionSide::Source)
        .expect("connections");
    assert!(a_source.contains_key(&eid));

    let a_target = lookups.connections_for_node_side(a, ConnectionSide::Target);
    assert!(a_target.is_none() || !a_target.unwrap().contains_key(&eid));

    let b_target = lookups
        .connections_for_node_side(b, ConnectionSide::Target)
        .expect("connections");
    assert!(b_target.contains_key(&eid));

    let b_source = lookups.connections_for_node_side(b, ConnectionSide::Source);
    assert!(b_source.is_none() || !b_source.unwrap().contains_key(&eid));

    let _ = (out_port, in_port);
}
