use syn::{Path, ExprMatch, fold::{Fold, fold_expr_match}};

pub struct PatternBuilder {
    node_path: Path,
    max_depth: Option<u16>
}

impl PatternBuilder {
    pub fn new(node_path: Path) -> Self {
        Self { node_path, max_depth: None }
    }
}

impl Fold for PatternBuilder {
    fn fold_expr_match(&mut self, input: ExprMatch) -> ExprMatch {
        fold_expr_match(self, input)
    }
}
