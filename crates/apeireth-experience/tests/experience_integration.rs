//! Integration tests for apeireth-experience (post-1.0.0 增量)
//!
//! src/lib.rs mod tests 已覆盖 1 集成 case. 这里 (tests/) 加 per-行为样板.
//!
//! 真生产价值:
//! - WikiEntry 4 工厂方法 (new / with_tag / with_source / promote) 端到端
//! - KnowledgeGraph 边界 (empty / add_node 返 UUID / 2 node kinds / add_edge 校验)
//! - AssociationNetwork 联想传播 (seed / propagate / decay / associate) 端到端
//! - council_bridge 3 个 transform 函数端到端
//! - 集成: Wiki + KG + Association 跨模块协作
//!
//! 0 触碰 src/, 0 编造"已实现"。

#![allow(missing_docs)]

use apeireth_experience::{
    bundle_to_history_refs, kg_to_context_block, wiki_to_context_block, wiki_to_history_ref,
    AssociationEdge, AssociationNetwork, AssociationNode, KnowledgeEdge, KnowledgeGraph,
    KnowledgeNode, NodeKind, RelationKind, WikiEntry,
};

// =============================================================================
// 1. WikiEntry 工厂方法端到端
// =============================================================================

#[test]
fn wiki_entry_new_clamps_confidence() {
    let e1 = WikiEntry::new("t", "c", 1.5);
    assert_eq!(e1.confidence, 1.0, "confidence clamp 到 1.0");
    let e2 = WikiEntry::new("t", "c", -0.5);
    assert_eq!(e2.confidence, 0.0, "confidence clamp 到 0.0");
    let e3 = WikiEntry::new("t", "c", 0.7);
    assert_eq!(e3.confidence, 0.7, "0..1 范围保持");
}

#[test]
fn wiki_entry_factory_methods_chain() {
    let e = WikiEntry::new("Rust ownership", "Lending rules", 0.9)
        .with_tag("rust")
        .with_tag("borrow-checker")
        .with_source("ep-1")
        .with_source("ep-2");
    assert_eq!(e.title, "Rust ownership");
    assert_eq!(e.content, "Lending rules");
    assert_eq!(e.confidence, 0.9);
    assert_eq!(e.tags, vec!["rust", "borrow-checker"]);
    assert_eq!(e.source_episode_ids, vec!["ep-1", "ep-2"]);
    assert_eq!(e.promotion_count, 0);
}

#[test]
fn wiki_entry_promote_increments_count() {
    let mut e = WikiEntry::new("t", "c", 0.5);
    e.promote();
    assert_eq!(e.promotion_count, 1);
    e.promote();
    e.promote();
    assert_eq!(e.promotion_count, 3);
}

#[test]
fn wiki_entry_is_high_confidence_uses_threshold() {
    let e = WikiEntry::new("t", "c", 0.85);
    assert!(e.is_high_confidence(0.5), "0.85 >= 0.5");
    assert!(e.is_high_confidence(0.85), "边界 inclusive");
    assert!(!e.is_high_confidence(0.9), "0.85 < 0.9");
}

#[test]
fn wiki_entry_is_stale_uses_max_age() {
    let e = WikiEntry::new("t", "c", 0.5);
    assert!(
        !e.is_stale(i64::MAX),
        "刚创建应非 stale (max_age 远大于 age)"
    );
    assert!(e.is_stale(-1), "max_age < 0 永远 stale");
}

// =============================================================================
// 2. KnowledgeGraph 边界端到端
// =============================================================================

#[test]
fn knowledge_graph_empty_operations() {
    let kg = KnowledgeGraph::new();
    assert_eq!(kg.node_count(), 0);
    assert_eq!(kg.edge_count(), 0);
}

#[test]
fn knowledge_graph_add_node_returns_unique_uuid() {
    let mut kg = KnowledgeGraph::new();
    let n1 = kg.add_node(KnowledgeNode::new("a", NodeKind::Extracted, 0.5, "ep-1"));
    let n2 = kg.add_node(KnowledgeNode::new("b", NodeKind::Extracted, 0.5, "ep-1"));
    assert_ne!(n1, n2, "每次 add_node 应返不同 UUID");
    assert_eq!(kg.node_count(), 2);
}

#[test]
fn knowledge_graph_2_node_kinds_supported() {
    let mut kg = KnowledgeGraph::new();
    kg.add_node(KnowledgeNode::new(
        "extracted-1",
        NodeKind::Extracted,
        0.5,
        "ep-1",
    ));
    kg.add_node(KnowledgeNode::new(
        "inferred-1",
        NodeKind::Inferred,
        0.4,
        "ep-1",
    ));
    assert_eq!(kg.node_count(), 2);
}

#[test]
fn knowledge_graph_add_edge_validates_endpoints() {
    let mut kg = KnowledgeGraph::new();
    let a = kg.add_node(KnowledgeNode::new("a", NodeKind::Extracted, 0.5, "ep-1"));
    let b = kg.add_node(KnowledgeNode::new("b", NodeKind::Extracted, 0.5, "ep-1"));
    let result = kg.add_edge(KnowledgeEdge::new(a, b, RelationKind::Symbiosis, 0.8));
    assert!(result.is_ok());
    assert_eq!(kg.edge_count(), 1);
}

#[test]
fn knowledge_graph_node_kind_serializes_to_pascal_case() {
    // 默认 serde rename (无 #[serde(rename_all)]), variant 名直接序列化
    assert_eq!(
        serde_json::to_string(&NodeKind::Extracted).unwrap(),
        "\"Extracted\""
    );
    assert_eq!(
        serde_json::to_string(&NodeKind::Inferred).unwrap(),
        "\"Inferred\""
    );
}

#[test]
fn knowledge_graph_relation_kind_serializes_to_pascal_case() {
    // 默认 serde rename (无 #[serde(rename_all)]), variant 名直接序列化
    assert_eq!(
        serde_json::to_string(&RelationKind::Symbiosis).unwrap(),
        "\"Symbiosis\""
    );
    assert_eq!(
        serde_json::to_string(&RelationKind::Coordination).unwrap(),
        "\"Coordination\""
    );
    assert_eq!(
        serde_json::to_string(&RelationKind::Embedding).unwrap(),
        "\"Embedding\""
    );
    assert_eq!(
        serde_json::to_string(&RelationKind::SelfRelation).unwrap(),
        "\"SelfRelation\""
    );
}

// =============================================================================
// 3. AssociationNetwork 联想传播端到端
// =============================================================================

#[test]
fn association_network_default_empty() {
    let n = AssociationNetwork::new();
    assert_eq!(n.node_count(), 0);
}

#[test]
fn association_network_add_node_boosts_initial_energy() {
    let mut net = AssociationNetwork::new();
    let id = net.add_node(AssociationNode::new("topic", 0.7));
    let node = net.node(&id).expect("node 应存在");
    assert!((node.energy - 0.7).abs() < 1e-9, "energy = initial");
}

#[test]
fn association_network_add_node_clamps_energy_to_unit() {
    let mut net = AssociationNetwork::new();
    let id1 = net.add_node(AssociationNode::new("a", 1.5));
    let id2 = net.add_node(AssociationNode::new("b", -0.5));
    assert!(
        (net.node(&id1).unwrap().energy - 1.0).abs() < 1e-9,
        "1.5 → 1.0"
    );
    assert!(
        (net.node(&id2).unwrap().energy - 0.0).abs() < 1e-9,
        "-0.5 → 0.0"
    );
}

#[test]
fn association_network_connect_creates_edge() {
    let mut net = AssociationNetwork::new();
    let a = net.add_node(AssociationNode::new("source", 0.0));
    let b = net.add_node(AssociationNode::new("target", 0.0));
    net.connect(a, b, 0.5);
    let r = net.associate(a, 1);
    assert_eq!(r.len(), 2, "source + target 应都被返回");
}

#[test]
fn association_network_associate_propagates_energy() {
    let mut net = AssociationNetwork::new();
    let a = net.add_node(AssociationNode::new("source", 1.0));
    let b = net.add_node(AssociationNode::new("target", 0.0));
    net.connect(a, b, 1.0);
    let r = net.associate(a, 1);
    let target_energy = r
        .iter()
        .find(|(id, _, _)| *id == b)
        .map(|(_, _, e)| *e)
        .unwrap();
    assert!(
        target_energy > 0.0,
        "联想传播应激活 target: got {target_energy}"
    );
}

#[test]
fn association_network_empty_seed_returns_empty() {
    let mut net = AssociationNetwork::new();
    let result = net.associate(uuid::Uuid::new_v4(), 3);
    assert!(result.is_empty(), "空 network associate 返空");
}

#[test]
fn association_network_decay_reduces_energy() {
    let mut net = AssociationNetwork::new().with_decay(0.5);
    let id = net.add_node(AssociationNode::new("a", 1.0));
    net.decay_all();
    assert!((net.node(&id).unwrap().energy - 0.5).abs() < 1e-9);
}

#[test]
fn association_edge_weight_clamped_to_unit() {
    let e1 = AssociationEdge::new(1.5);
    assert_eq!(e1.weight, 1.0);
    let e2 = AssociationEdge::new(-0.5);
    assert_eq!(e2.weight, 0.0);
}

// =============================================================================
// 4. council_bridge 3 个 transform 函数端到端
// =============================================================================

#[test]
fn wiki_to_history_ref_includes_title() {
    let e = WikiEntry::new("Rust ownership", "Lending rules", 0.9);
    let r = wiki_to_history_ref(&e);
    assert!(r.contains("Rust ownership"), "history ref 应含 title: {r}");
}

#[test]
fn wiki_to_context_block_includes_title_and_content() {
    let e = WikiEntry::new("title-A", "body-B with some text", 0.9);
    let r = wiki_to_context_block(&e);
    assert!(r.contains("title-A"));
    assert!(r.contains("body-B"));
}

#[test]
fn kg_to_context_block_respects_max_nodes() {
    let mut kg = KnowledgeGraph::new();
    for i in 0..10 {
        kg.add_node(KnowledgeNode::new(
            format!("n-{i}"),
            NodeKind::Extracted,
            0.5,
            "ep-1",
        ));
    }
    let r = kg_to_context_block(&kg, 3);
    // 按 confidence 降序排, 10 个节点 confidence 一样, 所以按 stable 排序
    // 排序结果取前 3, 但具体哪些被取跟实现相关
    assert!(r.contains("[EX]"), "应含节点类型前缀");
    // 取 3 个 → 输出 3 行
    let ex_count = r.matches("[EX]").count();
    assert!(ex_count <= 3, "max_nodes=3 应 ≤ 3 个: got {ex_count}");
}

#[test]
fn kg_to_context_block_empty_kg_has_header() {
    let kg = KnowledgeGraph::new();
    let r = kg_to_context_block(&kg, 5);
    // 即使空 KG 也有 header (per council_bridge.rs:64)
    assert!(r.contains("Knowledge Graph"), "空 KG 也有 header");
    assert!(!r.contains("[EX]"), "空 KG 不应含节点");
}

#[test]
fn bundle_to_history_refs_combines_multiple_sources() {
    // bundle_to_history_refs(wiki, kg, seed_node, depth) -> Vec<String>
    let w1 = WikiEntry::new("a", "...", 0.9);
    let w2 = WikiEntry::new("b", "...", 0.8);
    let mut kg = KnowledgeGraph::new();
    let _n1 = kg.add_node(KnowledgeNode::new(
        "kg-node",
        NodeKind::Extracted,
        0.5,
        "ep-1",
    ));
    let refs = bundle_to_history_refs(&[&w1, &w2], &kg, _n1, 2);
    assert!(!refs.is_empty(), "bundle 应返非空");
    let joined = refs.join("|");
    assert!(joined.contains("a"), "bundle 应含 a: {joined}");
    assert!(joined.contains("b"), "bundle 应含 b: {joined}");
}

// =============================================================================
// 5. 集成: Wiki + KG + Association 跨模块
// =============================================================================

#[test]
fn integration_wiki_kg_association_flow() {
    // 1. Wiki entry 创建 + promote
    let mut wiki = WikiEntry::new("Rust ownership", "Lending rules", 0.9)
        .with_tag("rust")
        .with_source("ep-1");
    wiki.promote();
    assert_eq!(wiki.promotion_count, 1);

    // 2. KG 加 node
    let mut kg = KnowledgeGraph::new();
    let _n1 = kg.add_node(KnowledgeNode::new(
        &wiki.title,
        NodeKind::Extracted,
        0.9,
        "ep-1",
    ));

    // 3. Association 联想网络 (含传播)
    let mut assoc = AssociationNetwork::new();
    let a = assoc.add_node(AssociationNode::new(&wiki.title, 0.0));
    let b = assoc.add_node(AssociationNode::new("Borrow checker", 0.0));
    assoc.connect(a, b, 1.0);
    let r = assoc.associate(a, 2);
    assert_eq!(r.len(), 2);
    let borrow_energy = r
        .iter()
        .find(|(id, _, _)| *id == b)
        .map(|(_, _, e)| *e)
        .unwrap();
    assert!(borrow_energy > 0.0, "联想传播应激活 borrow checker");

    // 4. council_bridge 转换都能用
    let wiki_ctx = wiki_to_context_block(&wiki);
    let kg_ctx = kg_to_context_block(&kg, 10);
    let history_ref = wiki_to_history_ref(&wiki);

    assert!(wiki_ctx.contains("Rust ownership"));
    assert!(kg_ctx.contains("Rust ownership"));
    assert!(history_ref.contains("Rust ownership"));
}

#[test]
fn integration_wiki_lifecycle_promote_shows_in_context() {
    let mut wiki = WikiEntry::new("evolving topic", "initial", 0.5);
    let ctx0 = wiki_to_context_block(&wiki);
    assert!(ctx0.contains("promotions: 0"));
    assert!(ctx0.contains("initial"));

    wiki.promote();
    wiki.promote();
    wiki.promote();
    assert_eq!(wiki.promotion_count, 3);
    assert_eq!(wiki.content, "initial", "promote 不改 content");
    assert!(wiki.is_high_confidence(0.5));

    let ctx3 = wiki_to_context_block(&wiki);
    assert!(
        ctx3.contains("promotions: 3"),
        "升 3 次后 context_block 应含 promotions: 3"
    );
    assert!(ctx3.contains("initial"), "升 3 次后 content 仍 \"initial\"");
    assert_ne!(ctx0, ctx3, "promotions 字段变了, context_block 不同");
}
