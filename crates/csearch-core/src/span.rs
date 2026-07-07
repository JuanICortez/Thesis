// SpanAnalysis (richer Span: file_id, byte range, line/col)
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub fn slice<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }
}

#[derive(Clone, Debug)]
pub struct SpanTree {
    pub span: Span,
    pub children: Vec<SpanTree>,
}

impl SpanTree {
    pub fn leaf(span: Span) -> Self {
        SpanTree {
            span,
            children: Vec::new(),
        }
    }

    pub fn node(span: Span, children: Vec<SpanTree>) -> Self {
        SpanTree { span, children }
    }
}
