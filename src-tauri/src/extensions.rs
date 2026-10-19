//! Revocable, bounded lighting overlays shared by declarative plugins and telemetry.
use crate::{protocol::Rgb, studio::Composition};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub event: String,
    pub color: String,
    pub seconds: u8,
    pub start: u16,
    pub end: u16,
}
impl Rule {
    pub fn validate(&self) -> Result<(), String> {
        if !["damage", "kill", "roundStart", "bombPlanted"].contains(&self.event.as_str())
            || !crate::sharing::valid_color(&self.color)
            || !(1..=10).contains(&self.seconds)
            || self.start >= self.end
            || self.end > 1000
        {
            return Err(
                "Cue needs a supported event, #RRGGBB color, 1–10 seconds and a zone within 0–1000"
                    .into(),
            );
        }
        Ok(())
    }
}
pub struct Overlay {
    active: Arc<AtomicBool>,
    additional: Vec<Arc<AtomicBool>>,
    started: Instant,
    effect: Option<Composition>,
    cue: Option<Rule>,
}
impl Overlay {
    pub fn effect(effect: Composition, active: Arc<AtomicBool>) -> Result<Self, String> {
        effect.validate()?;
        Ok(Self {
            active,
            additional: vec![],
            started: Instant::now(),
            effect: Some(effect),
            cue: None,
        })
    }
    pub fn cue(cue: Rule, active: Arc<AtomicBool>) -> Result<Self, String> {
        cue.validate()?;
        Ok(Self {
            active,
            additional: vec![],
            started: Instant::now(),
            effect: None,
            cue: Some(cue),
        })
    }
    pub fn with_guard(mut self, guard: Arc<AtomicBool>) -> Self {
        self.additional.push(guard);
        self
    }
    pub fn live(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && self.additional.iter().all(|g| g.load(Ordering::Acquire))
            && self
                .cue
                .as_ref()
                .is_none_or(|c| self.started.elapsed() < Duration::from_secs(c.seconds.into()))
    }
    pub fn render(&self, colors: &mut [Rgb], speed: u8, reverse: bool) {
        if !self.live() {
            return;
        }
        let count = colors.len();
        for (i, color) in colors.iter_mut().enumerate() {
            let position = (i as f32 + 0.5) / count as f32;
            if let Some(effect) = &self.effect {
                *color = effect.color(
                    position,
                    self.started.elapsed().as_secs_f32(),
                    speed,
                    reverse,
                );
            }
            if let Some(cue) = &self.cue {
                let point = position * 1000.0;
                if point >= cue.start as f32 && point < cue.end as f32 {
                    *color = std::array::from_fn(|n| {
                        u8::from_str_radix(&cue.color[1 + n * 2..3 + n * 2], 16).unwrap_or(0)
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zones_revocation_and_bounds_preserve_underlying_pixels() {
        let active = Arc::new(AtomicBool::new(true));
        let mut rule = Rule {
            event: "damage".into(),
            color: "#ff0000".into(),
            seconds: 1,
            start: 500,
            end: 1000,
        };
        let overlay = Overlay::cue(rule.clone(), active.clone()).unwrap();
        let mut colors = [[1; 3]; 4];
        overlay.render(&mut colors, 50, false);
        assert_eq!(colors, [[1; 3], [1; 3], [255, 0, 0], [255, 0, 0]]);
        active.store(false, Ordering::Release);
        colors.fill([2; 3]);
        overlay.render(&mut colors, 50, false);
        assert_eq!(colors, [[2; 3]; 4]);
        rule.end = 1001;
        assert!(rule.validate().is_err());
    }
}
