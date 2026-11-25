use super::*;

struct NoFacts;
impl Facts<u64> for NoFacts {
    fn display_none(&self, _: u64) -> bool {
        false
    }
    fn invisible(&self, _: u64) -> bool {
        false
    }
    fn natively_disabled(&self, _: u64) -> bool {
        false
    }
    fn editor_value(&self, _: u64) -> Option<String> {
        None
    }
    fn tabbable(&self, k: u64) -> bool {
        k == 2
    }
    fn checked(&self, _: u64) -> bool {
        false
    }
    fn test_id(&self, k: u64) -> Option<String> {
        (k == 2).then(|| "go".to_string())
    }
}

fn el(tag: &str, parent: Option<u64>, kids: Vec<u64>, text: Option<&str>, props: Value) -> El<u64> {
    El { tag: tag.into(), props: props.as_object().cloned().unwrap_or_default(), parent, kids, text: text.map(str::to_string), title: None }
}

/// raea's window publishes this tree (raea-page-host ax.rs): its root is the window, named; each element hangs under
/// it with its box, a button named by its text, and the focus where the page has it.
#[test]
fn the_window_tree_has_the_page_under_its_root_with_boxes() {
    let els = HashMap::from([
        (1, el("main", None, vec![2, 4], None, json!({}))),
        (2, el("button", Some(1), vec![3], None, json!({"data-testid": "go"}))),
        (3, el("text", Some(2), vec![], Some("Start project"), json!({}))),
        (4, el("text", Some(1), vec![], Some("\n    "), json!({}))),
    ]);
    let roots = HashMap::from([(1, 0), (2, 0), (3, 0), (4, 0)]);
    let window = NodeId(7);
    let (u, tested) = tree(els, roots, &NoFacts, Some(2), |k| k + 100, window, Some("raea"), |k| (k == 2).then_some([10.0, 20.0, 110.0, 50.0]));
    assert_eq!(tested, vec![("go".to_string(), NodeId(102))]);
    assert_eq!(u.focus, NodeId(102));
    assert_eq!(u.tree.as_ref().map(|t| t.root), Some(window));
    let (_, root) = u.nodes.iter().find(|(id, _)| *id == window).unwrap();
    assert_eq!(root.label(), Some("raea"));
    assert_eq!(root.children(), &[NodeId(101)]);
    // the markup's white space is no node
    assert!(u.nodes.iter().all(|(id, _)| *id != NodeId(104)));
    let (_, main) = u.nodes.iter().find(|(id, _)| *id == NodeId(101)).unwrap();
    assert_eq!(main.children(), &[NodeId(102)]);
    let state = accesskit_consumer::Tree::new(u, true);
    let b = state.state().node_by_tree_local_id(NodeId(102), TreeId::ROOT).unwrap();
    assert_eq!(b.role(), Role::Button);
    assert!(b.data().supports_action(Action::Click), "a button takes a press");
    assert_eq!(b.label().as_deref(), Some("Start project"));
    assert_eq!(b.data().bounds(), Some(accesskit::Rect { x0: 10.0, y0: 20.0, x1: 110.0, y1: 50.0 }));
}

#[test]
fn a_field_takes_a_value_unless_it_is_read_only() {
    let els = HashMap::from([(1, el("input", None, vec![], None, json!({"aria-label": "Title"}))), (2, el("input", None, vec![], None, json!({"readOnly": true})))]);
    let roots = HashMap::from([(1, 0), (2, 0)]);
    let (u, _) = tree(els, roots, &NoFacts, None, |k| k + 100, NodeId(7), None, |_| None);
    let field = |id| u.nodes.iter().find(|(i, _)| *i == NodeId(id)).unwrap().1.clone();
    assert!(field(101).supports_action(Action::SetValue));
    assert!(!field(102).supports_action(Action::SetValue));
}

/// AccessKit's filtered walk (what macOS lists) stops at a hidden node after an empty container, and what follows
/// is gone: unhide takes hidden nodes out so it isn't. When the first assert fails AccessKit has fixed it.
#[test]
fn what_follows_an_empty_container_and_a_hidden_one_is_listed_once_hidden_nodes_are_out() {
    use accesskit::Tree;
    let (win, body, layer, panel, inside, welcome, nav) = (NodeId(1), NodeId(2), NodeId(3), NodeId(4), NodeId(5), NodeId(6), NodeId(8));
    let named = |r, l: &str| {
        let mut n = AkNode::new(r);
        n.set_label(l);
        n
    };
    let update = || {
        let mut root = AkNode::new(Role::Window);
        root.set_children(vec![body]);
        let mut b = AkNode::new(Role::GenericContainer);
        // the rail, an empty layer, the closed side panel (hidden), raea.app's welcome
        b.set_children(vec![nav, layer, panel, welcome]);
        let mut p = AkNode::new(Role::Complementary);
        p.set_hidden();
        p.set_children(vec![inside]);
        TreeUpdate {
            nodes: vec![(win, root), (body, b), (layer, AkNode::new(Role::GenericContainer)), (panel, p), (inside, AkNode::new(Role::Button)), (welcome, named(Role::Button, "Welcome")), (nav, named(Role::Navigation, "Rail"))],
            tree: Some(Tree::new(win)),
            tree_id: TreeId::ROOT,
            focus: win,
        }
    };
    let listed = |u: TreeUpdate| {
        let t = accesskit_consumer::Tree::new(u, true);
        let root = t.state().root();
        root.filtered_children(accesskit_consumer::common_filter).filter_map(|n| n.label()).collect::<Vec<_>>()
    };
    assert_eq!(listed(update()), vec!["Rail".to_string()], "AccessKit lists what follows now: unhide may go");
    let mut u = update();
    unhide(&mut u, win);
    assert_eq!(listed(u), vec!["Rail".to_string(), "Welcome".to_string()], "the welcome is listed");
}

/// A name's parts are spaced by how they're laid out, where the host says: spans styled as blocks don't run together.
#[test]
fn a_name_spaces_parts_laid_out_as_blocks() {
    struct Blocks(bool);
    impl Facts<u64> for Blocks {
        fn display_none(&self, _: u64) -> bool { false }
        fn invisible(&self, _: u64) -> bool { false }
        fn natively_disabled(&self, _: u64) -> bool { false }
        fn editor_value(&self, _: u64) -> Option<String> { None }
        fn tabbable(&self, _: u64) -> bool { false }
        fn checked(&self, _: u64) -> bool { false }
        fn test_id(&self, _: u64) -> Option<String> { None }
        fn block(&self, _: u64) -> Option<bool> { Some(self.0) }
    }
    let name = |block| {
        let els = HashMap::from([
            (1, el("button", None, vec![2, 4], None, json!({}))),
            (2, el("span", Some(1), vec![3], None, json!({}))),
            (3, el("text", Some(2), vec![], Some("Sonnet 5.5"), json!({}))),
            (4, el("span", Some(1), vec![5], None, json!({}))),
            (5, el("text", Some(4), vec![], Some("Most efficient"), json!({}))),
        ]);
        let roots = (1..=5).map(|k| (k, 0)).collect();
        let (u, _) = tree(els, roots, &Blocks(block), None, |k| k + 100, NodeId(7), None, |_| None);
        u.nodes.iter().find(|(i, _)| *i == NodeId(101)).unwrap().1.label().map(str::to_string)
    };
    assert_eq!(name(true).as_deref(), Some("Sonnet 5.5 Most efficient"));
    assert_eq!(name(false).as_deref(), Some("Sonnet 5.5Most efficient"));
}
