use crate::ids::StationId;
use crate::units::People;

/// 行 × 目的駅の人数表。列数は `max_stations` で固定し、行は追加だけ(10 TD-31)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaxMatrix {
    cols: u32,
    data: Vec<u64>,
}

impl PaxMatrix {
    pub fn new(cols: u32) -> Self {
        assert!(cols >= 1, "PaxMatrix の列数が 0");
        PaxMatrix {
            cols,
            data: Vec::new(),
        }
    }

    /// 行優先の全要素から作る(`Game::restore`)。`data.len()` が `cols` の倍数でなければ panic する。
    pub fn from_data(cols: u32, data: Vec<u64>) -> Self {
        assert!(
            cols >= 1 && data.len().is_multiple_of(cols as usize),
            "PaxMatrix の大きさ"
        );
        PaxMatrix { cols, data }
    }

    pub fn cols(&self) -> u32 {
        self.cols
    }

    pub fn rows(&self) -> u32 {
        u32::try_from(self.data.len() / self.cols as usize).expect("行数が u32 を超えた")
    }

    /// 0 人の行を足す(駅の出現・列車の追加)。
    pub fn push_row(&mut self) {
        self.data.resize(self.data.len() + self.cols as usize, 0);
    }

    fn at(&self, row: u32, dest: StationId) -> usize {
        assert!(
            dest.0 < self.cols,
            "目的駅 {dest} が列数 {} の外",
            self.cols
        );
        row as usize * self.cols as usize + dest.index()
    }

    pub fn get(&self, row: u32, dest: StationId) -> People {
        People(self.data[self.at(row, dest)])
    }

    /// 足す。あふれたら飽和し、デバッグビルドでは止める(10 TD-21)。
    pub fn add(&mut self, row: u32, dest: StationId, n: People) {
        let i = self.at(row, dest);
        self.data[i] = People(self.data[i]).add_sat(n).0;
    }

    /// 最大 `n` 人を取り出し、実際に取り出した人数を返す。
    pub fn take(&mut self, row: u32, dest: StationId, n: People) -> People {
        let i = self.at(row, dest);
        let taken = n.0.min(self.data[i]);
        self.data[i] -= taken;
        People(taken)
    }

    /// 1 行(目的駅の昇順)。
    pub fn row(&self, row: u32) -> &[u64] {
        let start = row as usize * self.cols as usize;
        &self.data[start..start + self.cols as usize]
    }

    pub fn row_total(&self, row: u32) -> People {
        self.row(row)
            .iter()
            .fold(People::ZERO, |acc, &v| acc.add_sat(People(v)))
    }

    /// 全要素(行優先。state_hash 用)。
    pub fn as_slice(&self) -> &[u64] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_add_take() {
        let mut m = PaxMatrix::new(3);
        assert_eq!(m.rows(), 0);
        m.push_row();
        m.push_row();
        assert_eq!((m.rows(), m.cols()), (2, 3));
        m.add(1, StationId(2), People(5));
        m.add(1, StationId(0), People(1));
        assert_eq!(m.get(1, StationId(2)), People(5));
        assert_eq!(m.row(1), &[1, 0, 5]);
        assert_eq!(m.row(0), &[0, 0, 0]);
        assert_eq!(m.row_total(1), People(6));
        assert_eq!(m.take(1, StationId(2), People(3)), People(3));
        assert_eq!(m.take(1, StationId(2), People(10)), People(2));
        assert_eq!(m.as_slice(), &[0, 0, 0, 1, 0, 0]);
    }

    #[test]
    #[should_panic(expected = "列数")]
    fn dest_out_of_range_panics() {
        let mut m = PaxMatrix::new(2);
        m.push_row();
        m.get(0, StationId(2));
    }
}
