//! Maximum flow (Dinic) with `i128` capacities, for the closure problem in
//! `growth`.

pub(crate) const INFINITE: i128 = i128::MAX / 4;

pub(crate) struct Network {
    /// Per edge: head, residual capacity. Edge `e ^ 1` is the reverse of `e`.
    head: Vec<usize>,
    capacity: Vec<i128>,
    edges_from: Vec<Vec<usize>>,
}

impl Network {
    pub(crate) fn new(nodes: usize) -> Self {
        Self {
            head: Vec::new(),
            capacity: Vec::new(),
            edges_from: vec![Vec::new(); nodes],
        }
    }

    pub(crate) fn add_edge(&mut self, from: usize, to: usize, capacity: i128) {
        self.edges_from[from].push(self.head.len());
        self.head.push(to);
        self.capacity.push(capacity);
        self.edges_from[to].push(self.head.len());
        self.head.push(from);
        self.capacity.push(0);
    }

    pub(crate) fn max_flow(&mut self, source: usize, sink: usize) -> i128 {
        let nodes = self.edges_from.len();
        let mut total = 0;
        loop {
            // Breadth-first levels from the source over residual edges.
            let mut level = vec![usize::MAX; nodes];
            level[source] = 0;
            let mut queue = std::collections::VecDeque::from([source]);
            while let Some(node) = queue.pop_front() {
                for &edge in &self.edges_from[node] {
                    let next = self.head[edge];
                    if self.capacity[edge] > 0 && level[next] == usize::MAX {
                        level[next] = level[node] + 1;
                        queue.push_back(next);
                    }
                }
            }
            if level[sink] == usize::MAX {
                return total;
            }
            let mut next_edge = vec![0; nodes];
            loop {
                let pushed = self.augment(source, sink, INFINITE, &level, &mut next_edge);
                if pushed == 0 {
                    break;
                }
                total += pushed;
            }
        }
    }

    fn augment(
        &mut self,
        node: usize,
        sink: usize,
        limit: i128,
        level: &[usize],
        next_edge: &mut [usize],
    ) -> i128 {
        if node == sink {
            return limit;
        }
        while next_edge[node] < self.edges_from[node].len() {
            let edge = self.edges_from[node][next_edge[node]];
            let next = self.head[edge];
            if self.capacity[edge] > 0 && level[next] == level[node] + 1 {
                let pushed =
                    self.augment(next, sink, limit.min(self.capacity[edge]), level, next_edge);
                if pushed > 0 {
                    self.capacity[edge] -= pushed;
                    self.capacity[edge ^ 1] += pushed;
                    return pushed;
                }
            }
            next_edge[node] += 1;
        }
        0
    }
}
