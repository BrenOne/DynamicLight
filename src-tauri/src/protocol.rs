pub type Rgb = [u8; 3];
pub const VID: u16 = 0x1a86;
pub const PID: u16 = 0xfe07;

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))
}
pub fn command(id: u8, action: u8, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= 249);
    let mut bytes = vec![0x52, 0x42, (payload.len() + 6) as u8, id, action];
    bytes.extend_from_slice(payload);
    bytes.push(checksum(&bytes));
    bytes
}
pub fn solid(id: u8, rgb: Rgb, count: usize) -> Vec<u8> {
    assert!((1..=127).contains(&count));
    command(
        id,
        0x86,
        &[
            1,
            rgb[0],
            rgb[1],
            rgb[2],
            count as u8,
            count as u8 + 1,
            0,
            0,
            0,
            254,
        ],
    )
}
/// Ranges per frame. The official app and OpenRGB send at most 34 ranges;
/// larger frames make the controller stall USB writes (hidapi 0x3E5 timeouts).
pub const MAX_RANGES: usize = 34;

/// Full-strip update (0x80) using 1-based inclusive (start, r, g, b, end) ranges,
/// the same layout as the official app. Adjacent LEDs are merged greedily with
/// the smallest colour error until at most MAX_RANGES remain.
pub fn frame(id: u8, colors: &[Rgb]) -> Vec<u8> {
    assert!((1..=127).contains(&colors.len()));
    let ranges = merge(colors, MAX_RANGES);
    let len = 7 + ranges.len() * 5;
    let mut bytes = vec![0x53, 0x43, (len >> 8) as u8, len as u8, id, 0x80];
    for (start, end, rgb) in ranges {
        bytes.extend_from_slice(&[start, rgb[0], rgb[1], rgb[2], end]);
    }
    bytes.push(checksum(&bytes));
    bytes
}

struct Range {
    start: usize,
    end: usize,
    n: f64,
    sum: [f64; 3],
    term: f64,
}
fn term(sum: &[f64; 3], n: f64) -> f64 {
    (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]) / n
}
/// Returns (start, end, colour) with 1-based inclusive LED indices.
pub fn merge(colors: &[Rgb], limit: usize) -> Vec<(u8, u8, Rgb)> {
    let mut ranges: Vec<Range> = colors
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let sum = [c[0] as f64, c[1] as f64, c[2] as f64];
            Range { start: i + 1, end: i + 1, n: 1.0, term: term(&sum, 1.0), sum }
        })
        .collect();
    while ranges.len() > limit.max(1) {
        let (mut best, mut best_delta, mut best_term) = (0, f64::MAX, 0.0);
        for i in 0..ranges.len() - 1 {
            let (a, b) = (&ranges[i], &ranges[i + 1]);
            let sum = [a.sum[0] + b.sum[0], a.sum[1] + b.sum[1], a.sum[2] + b.sum[2]];
            let t = term(&sum, a.n + b.n);
            let delta = a.term + b.term - t;
            if delta < best_delta {
                (best, best_delta, best_term) = (i, delta, t);
            }
        }
        let right = ranges.remove(best + 1);
        let left = &mut ranges[best];
        left.end = right.end;
        left.n += right.n;
        for c in 0..3 {
            left.sum[c] += right.sum[c];
        }
        left.term = best_term;
    }
    ranges
        .iter()
        .map(|r| {
            let avg = |c: usize| (r.sum[c] / r.n).round().clamp(0.0, 255.0) as u8;
            (r.start as u8, r.end as u8, [avg(0), avg(1), avg(2)])
        })
        .collect()
}
pub fn reports(packet: &[u8]) -> impl Iterator<Item = [u8; 65]> + '_ {
    packet.chunks(64).map(|chunk| {
        let mut report = [0; 65];
        report[1..1 + chunk.len()].copy_from_slice(chunk);
        report
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_init_packets() {
        assert_eq!(command(2, 0x82, &[]), [0x52, 0x42, 6, 2, 0x82, 0x1e]);
        assert_eq!(command(3, 0x93, &[0]), [0x52, 0x42, 7, 3, 0x93, 0, 0x31]);
    }
    #[test]
    fn solid_clears_unused_leds() {
        let p = solid(1, [255, 0, 128], 65);
        assert_eq!(&p[5..15], &[1, 255, 0, 128, 65, 66, 0, 0, 0, 254]);
        assert_eq!(p[15], checksum(&p[..15]));
    }
    #[test]
    fn full_frame_roundtrips_reports() {
        for n in [1, 34, 94, 127] {
            let colors: Vec<_> = (0..n).map(|i| [i as u8, 255 - i as u8, 128]).collect();
            let p = frame(7, &colors);
            let k = n.min(MAX_RANGES);
            assert_eq!(p.len(), k * 5 + 7);
            assert_eq!(u16::from_be_bytes([p[2], p[3]]) as usize, p.len());
            assert_eq!(&p[4..6], &[7, 0x80]);
            assert_eq!(p[6], 1);
            assert_eq!(p[p.len() - 2], n as u8);
            assert_eq!(p[p.len() - 1], checksum(&p[..p.len() - 1]));
            let chunks: Vec<_> = reports(&p).collect();
            assert!(chunks.iter().all(|r| r[0] == 0));
            let joined: Vec<_> = chunks.iter().flat_map(|r| r[1..].iter().copied()).collect();
            assert_eq!(&joined[..p.len()], &p);
        }
    }
    #[test]
    fn merge_covers_every_led_and_keeps_detail() {
        let mut colors = vec![[0, 0, 0]; 94];
        colors[40] = [255, 255, 255];
        let r = merge(&colors, 34);
        assert!(r.len() <= 34);
        assert_eq!(r[0].0, 1);
        assert_eq!(r.last().unwrap().1, 94);
        assert!(r.windows(2).all(|w| w[1].0 == w[0].1 + 1));
        assert!(r.iter().any(|x| x.0 == 41 && x.1 == 41 && x.2 == [255, 255, 255]));
    }
}
