use std::sync::OnceLock;

static INSTANCE: OnceLock<Context> = OnceLock::new();

pub struct Context {
    lines_before: Vec<String>,
    lines_after: Vec<String>,
    before_count: usize,
    after_count: usize,
    before_max: usize,
    after_max: usize,
}

impl Context {
    pub fn global() -> &'static Context {
        INSTANCE.get_or_init(|| Context::new(5, 5))
    }

    pub fn new(before: usize, after: usize) -> Self {
        let bm = before;
        let am = after;
        Context {
            lines_before: if bm > 0 {
                vec![String::new(); bm]
            } else {
                Vec::new()
            },
            lines_after: if am > 0 {
                vec![String::new(); am]
            } else {
                Vec::new()
            },
            before_count: 0,
            after_count: 0,
            before_max: bm,
            after_max: am,
        }
    }

    pub fn add_line(&mut self, line: &str) {
        if self.after_max > 0 && self.after_count < self.after_max {
            self.lines_after[self.after_count] = line.to_string();
            self.after_count += 1;
        }
    }

    pub fn clear(&mut self) {
        if self.before_max > 0 {
            self.lines_before = vec![String::new(); self.before_max];
            self.before_count = 0;
        }
        if self.after_max > 0 {
            self.lines_after = vec![String::new(); self.after_max];
            self.after_count = 0;
        }
    }

    pub fn lines_before(&self) -> &[String] {
        &self.lines_before[..self.before_count]
    }

    pub fn lines_after(&self) -> &[String] {
        &self.lines_after[..self.after_count]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create() {
        let ctx = Context::new(3, 3);
        assert_eq!(ctx.lines_before().len(), 0);
        assert_eq!(ctx.lines_after().len(), 0);
    }

    #[test]
    fn test_add_line() {
        let mut ctx = Context::new(3, 3);
        ctx.add_line("hello");
        assert_eq!(ctx.lines_after().len(), 1);
        assert_eq!(ctx.lines_after()[0], "hello");
    }

    #[test]
    fn test_clear() {
        let mut ctx = Context::new(3, 3);
        ctx.add_line("hello");
        ctx.clear();
        assert_eq!(ctx.lines_after().len(), 0);
    }

    #[test]
    fn test_zero_bounds() {
        let mut ctx = Context::new(0, 0);
        ctx.add_line("hello");
        assert_eq!(ctx.lines_after().len(), 0);
    }

    #[test]
    fn test_max_capacity() {
        let mut ctx = Context::new(2, 2);
        ctx.add_line("a");
        ctx.add_line("b");
        ctx.add_line("c");
        assert_eq!(ctx.lines_after().len(), 2);
    }
}
