//! 直線の ADSR エンベロープ(Q16、0..=65535)。

const TOP: u32 = 65_535;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Off,
}

/// 時間はサンプル数で持つ。
#[derive(Clone, Copy, Debug)]
pub(crate) struct Env {
    attack: u32,
    decay: u32,
    sustain: u32,
    release: u32,
    stage: Stage,
    n: u32,
    from: u32,
}

impl Env {
    pub(crate) fn new(attack: u32, decay: u32, sustain_pct: u8, release: u32) -> Env {
        let mut e = Env {
            attack,
            decay,
            sustain: TOP * u32::from(sustain_pct) / 100,
            release,
            stage: Stage::Attack,
            n: 0,
            from: 0,
        };
        e.settle();
        e
    }

    fn settle(&mut self) {
        loop {
            let len = match self.stage {
                Stage::Attack => self.attack,
                Stage::Decay => self.decay,
                Stage::Release => self.release,
                Stage::Sustain | Stage::Off => return,
            };
            if self.n < len {
                return;
            }
            self.n = 0;
            self.stage = match self.stage {
                Stage::Attack => Stage::Decay,
                Stage::Decay => Stage::Sustain,
                _ => Stage::Off,
            };
        }
    }

    pub(crate) fn value(&self) -> u32 {
        match self.stage {
            Stage::Attack => TOP * self.n / self.attack,
            Stage::Decay => TOP - (TOP - self.sustain) * self.n / self.decay,
            Stage::Sustain => self.sustain,
            Stage::Release => self.from - self.from * self.n / self.release,
            Stage::Off => 0,
        }
    }

    pub(crate) fn advance(&mut self) {
        if matches!(self.stage, Stage::Sustain | Stage::Off) {
            return;
        }
        self.n += 1;
        self.settle();
    }

    pub(crate) fn release(&mut self) {
        if self.stage == Stage::Off {
            return;
        }
        self.from = self.value();
        self.stage = Stage::Release;
        self.n = 0;
        self.settle();
    }

    pub(crate) fn is_off(&self) -> bool {
        self.stage == Stage::Off
    }
}

pub(crate) fn quantize(v: u32) -> u32 {
    (v >> 12) << 12
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(e: &mut Env, n: u32) -> Vec<u32> {
        (0..n)
            .map(|_| {
                let v = e.value();
                e.advance();
                v
            })
            .collect()
    }

    #[test]
    fn a04_breakpoints() {
        let mut e = Env::new(4, 10, 50, 8);
        let v = run(&mut e, 20);
        assert_eq!(&v[..4], &[0, 16383, 32767, 49151]);
        assert_eq!(v[4], TOP);
        assert_eq!(v[9], TOP - (TOP - 32767) * 5 / 10);
        assert_eq!(v[13], TOP - (TOP - 32767) * 9 / 10);
        assert_eq!(v[14], 32767);
        assert_eq!(v[19], 32767);
        e.release();
        let r = run(&mut e, 9);
        assert_eq!(r[0], 32767);
        assert_eq!(r[4], 32767 - 32767 * 4 / 8);
        assert_eq!(r[7], 32767 - 32767 * 7 / 8);
        assert_eq!(r[8], 0);
        assert!(e.is_off());
    }

    #[test]
    fn a04_zero_lengths() {
        let mut e = Env::new(0, 0, 70, 0);
        assert_eq!(e.value(), TOP * 70 / 100);
        e.release();
        assert!(e.is_off());
        assert_eq!(e.value(), 0);

        let mut e = Env::new(0, 3, 0, 5);
        assert_eq!(
            run(&mut e, 4),
            vec![TOP, TOP - TOP / 3, TOP - TOP * 2 / 3, 0]
        );
    }

    #[test]
    fn a04_release_during_attack() {
        let mut e = Env::new(10, 0, 100, 2);
        run(&mut e, 5);
        e.release();
        assert_eq!(run(&mut e, 3), vec![TOP * 5 / 10, TOP * 5 / 10 / 2 + 1, 0]);
    }

    #[test]
    fn a04_quantize_16_steps() {
        let mut e = Env::new(1000, 0, 100, 0);
        let mut levels: Vec<u32> = run(&mut e, 1000).into_iter().map(quantize).collect();
        levels.dedup();
        assert_eq!(levels.len(), 16);
        assert_eq!(levels.first(), Some(&0));
        assert_eq!(levels.last(), Some(&(15 << 12)));
        assert!(levels.iter().all(|v| v % 4096 == 0));
    }
}
