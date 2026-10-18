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
pub fn frame(id: u8, colors: &[Rgb]) -> Vec<u8> {
    assert!((1..=127).contains(&colors.len()));
    let len = 7 + colors.len() * 5;
    let mut bytes = vec![0x53, 0x43, (len >> 8) as u8, len as u8, id];
    for (i, rgb) in colors.iter().enumerate() {
        bytes.extend_from_slice(&[
            if i == 0 { 0x80 } else { i as u8 },
            i as u8,
            rgb[0],
            rgb[1],
            rgb[2],
        ]);
    }
    bytes.push(colors.len() as u8);
    bytes.push(checksum(&bytes));
    bytes
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
        for n in [1, 65, 127] {
            let colors: Vec<_> = (0..n).map(|i| [i as u8, 255 - i as u8, 128]).collect();
            let p = frame(7, &colors);
            assert_eq!(p.len(), n * 5 + 7);
            assert_eq!(u16::from_be_bytes([p[2], p[3]]) as usize, p.len());
            assert_eq!(&p[5..10], &[128, 0, 0, 255, 128]);
            assert_eq!(p[p.len() - 2], n as u8);
            assert_eq!(p[p.len() - 1], checksum(&p[..p.len() - 1]));
            let chunks: Vec<_> = reports(&p).collect();
            assert!(chunks.iter().all(|r| r[0] == 0));
            let joined: Vec<_> = chunks.iter().flat_map(|r| r[1..].iter().copied()).collect();
            assert_eq!(&joined[..p.len()], &p);
            assert!(joined[p.len()..].iter().all(|b| *b == 0));
        }
    }
}
