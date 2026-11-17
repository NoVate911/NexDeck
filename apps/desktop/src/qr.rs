//! Self-contained ISO/IEC 18004 QR Code matrix generator (Byte Mode, Level L).
//!
//! Renders pairing payloads (`PairingQrPayload`) into a boolean module grid
//! for crisp vector drawing in `egui` without adding external dependencies.

/// Encoded QR code grid (`size x size` dark/light modules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrMatrix {
    size: usize,
    modules: Vec<bool>,
}

impl QrMatrix {
    /// Encodes `data` into a Level-L QR code (Versions 1..=10, up to 271 bytes).
    #[must_use]
    pub fn encode_bytes(data: &[u8]) -> Option<Self> {
        let spec = select_version_spec(data.len())?;
        let data_codewords = build_data_codewords(data, spec)?;
        let all_codewords = interleave_with_ecc(&data_codewords, spec);

        let size = 17 + spec.version * 4;
        let mut grid = vec![false; size * size];
        let mut is_function = vec![false; size * size];

        draw_function_patterns(&mut grid, &mut is_function, size, spec.version);
        draw_codewords(&mut grid, &is_function, size, &all_codewords);

        // Evaluate all 8 masks and pick the lowest penalty score.
        let mut best_mask = 0_u8;
        let mut best_penalty = u32::MAX;
        let mut best_grid = grid.clone();

        for mask in 0..8_u8 {
            let mut candidate = grid.clone();
            apply_mask(&mut candidate, &is_function, size, mask);
            draw_format_bits(&mut candidate, size, mask);
            let penalty = compute_penalty(&candidate, size);
            if penalty < best_penalty {
                best_penalty = penalty;
                best_mask = mask;
                best_grid = candidate;
            }
        }
        let _ = best_mask;

        Some(Self {
            size,
            modules: best_grid,
        })
    }

    /// Side dimension of the square QR matrix in modules.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Returns `true` if module `(x, y)` is dark.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x < self.size && y < self.size {
            self.modules[y * self.size + x]
        } else {
            false
        }
    }
}

#[derive(Clone, Copy)]
struct VersionSpec {
    version: usize,
    total_data_codewords: usize,
    ec_codewords_per_block: usize,
    num_blocks_g1: usize,
    data_per_block_g1: usize,
    num_blocks_g2: usize,
    data_per_block_g2: usize,
}

const VERSION_SPECS_L: &[VersionSpec] = &[
    VersionSpec {
        version: 1,
        total_data_codewords: 19,
        ec_codewords_per_block: 7,
        num_blocks_g1: 1,
        data_per_block_g1: 19,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 2,
        total_data_codewords: 34,
        ec_codewords_per_block: 10,
        num_blocks_g1: 1,
        data_per_block_g1: 34,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 3,
        total_data_codewords: 55,
        ec_codewords_per_block: 15,
        num_blocks_g1: 1,
        data_per_block_g1: 55,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 4,
        total_data_codewords: 80,
        ec_codewords_per_block: 20,
        num_blocks_g1: 1,
        data_per_block_g1: 80,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 5,
        total_data_codewords: 108,
        ec_codewords_per_block: 26,
        num_blocks_g1: 1,
        data_per_block_g1: 108,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 6,
        total_data_codewords: 136,
        ec_codewords_per_block: 18,
        num_blocks_g1: 2,
        data_per_block_g1: 68,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 7,
        total_data_codewords: 156,
        ec_codewords_per_block: 20,
        num_blocks_g1: 2,
        data_per_block_g1: 78,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 8,
        total_data_codewords: 194,
        ec_codewords_per_block: 24,
        num_blocks_g1: 2,
        data_per_block_g1: 97,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 9,
        total_data_codewords: 232,
        ec_codewords_per_block: 30,
        num_blocks_g1: 2,
        data_per_block_g1: 116,
        num_blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionSpec {
        version: 10,
        total_data_codewords: 274,
        ec_codewords_per_block: 18,
        num_blocks_g1: 2,
        data_per_block_g1: 68,
        num_blocks_g2: 2,
        data_per_block_g2: 69,
    },
];

fn select_version_spec(byte_len: usize) -> Option<VersionSpec> {
    for &spec in VERSION_SPECS_L {
        let count_bits = if spec.version <= 9 { 8 } else { 16 };
        let needed_bits = 4 + count_bits + byte_len * 8;
        if needed_bits <= spec.total_data_codewords * 8 {
            return Some(spec);
        }
    }
    None
}

fn build_data_codewords(data: &[u8], spec: VersionSpec) -> Option<Vec<u8>> {
    let capacity_bits = spec.total_data_codewords * 8;
    let mut bits = Vec::with_capacity(capacity_bits);

    // Byte mode indicator (0100)
    push_bits(&mut bits, 0b0100, 4);
    let count_bits = if spec.version <= 9 { 8 } else { 16 };
    push_bits(&mut bits, u32::try_from(data.len()).ok()?, count_bits);
    for &b in data {
        push_bits(&mut bits, u32::from(b), 8);
    }

    let terminator = (capacity_bits.saturating_sub(bits.len())).min(4);
    push_bits(&mut bits, 0, terminator);
    while bits.len() % 8 != 0 {
        bits.push(false);
    }

    let mut codewords = Vec::with_capacity(spec.total_data_codewords);
    for chunk in bits.as_chunks::<8>().0 {
        let mut byte = 0_u8;
        for &bit in chunk {
            byte = (byte << 1) | u8::from(bit);
        }
        codewords.push(byte);
    }

    let pad_bytes = [0xEC_u8, 0x11_u8];
    let mut pad_idx = 0;
    while codewords.len() < spec.total_data_codewords {
        codewords.push(pad_bytes[pad_idx % 2]);
        pad_idx += 1;
    }

    Some(codewords)
}

fn push_bits(out: &mut Vec<bool>, val: u32, count: usize) {
    for i in (0..count).rev() {
        out.push(((val >> i) & 1) != 0);
    }
}

fn interleave_with_ecc(data: &[u8], spec: VersionSpec) -> Vec<u8> {
    let mut data_blocks: Vec<Vec<u8>> = Vec::new();
    let mut ec_blocks: Vec<Vec<u8>> = Vec::new();
    let generator = reed_solomon_generator(spec.ec_codewords_per_block);

    let mut offset = 0;
    for _ in 0..spec.num_blocks_g1 {
        let block = data[offset..offset + spec.data_per_block_g1].to_vec();
        offset += spec.data_per_block_g1;
        ec_blocks.push(reed_solomon_remainder(&block, &generator));
        data_blocks.push(block);
    }
    for _ in 0..spec.num_blocks_g2 {
        let block = data[offset..offset + spec.data_per_block_g2].to_vec();
        offset += spec.data_per_block_g2;
        ec_blocks.push(reed_solomon_remainder(&block, &generator));
        data_blocks.push(block);
    }

    let max_data_len = spec.data_per_block_g1.max(spec.data_per_block_g2);
    let mut result = Vec::new();
    for i in 0..max_data_len {
        for block in &data_blocks {
            if let Some(&b) = block.get(i) {
                result.push(b);
            }
        }
    }
    for i in 0..spec.ec_codewords_per_block {
        for block in &ec_blocks {
            if let Some(&b) = block.get(i) {
                result.push(b);
            }
        }
    }
    result
}

fn gf_mul(mut x: u8, mut y: u8) -> u8 {
    let mut z = 0_u8;
    for _ in 0..8 {
        if (y & 1) != 0 {
            z ^= x;
        }
        let hi = (x & 0x80) != 0;
        x <<= 1;
        if hi {
            x ^= 0x1D;
        }
        y >>= 1;
    }
    z
}

fn reed_solomon_generator(degree: usize) -> Vec<u8> {
    let mut result = vec![0_u8; degree];
    result[degree - 1] = 1;
    let mut root = 1_u8;
    for _ in 0..degree {
        for j in 0..degree {
            result[j] = gf_mul(result[j], root);
            if j + 1 < degree {
                result[j] ^= result[j + 1];
            }
        }
        root = gf_mul(root, 0x02);
    }
    result
}

fn reed_solomon_remainder(data: &[u8], generator: &[u8]) -> Vec<u8> {
    let mut result = vec![0_u8; generator.len()];
    for &b in data {
        let factor = b ^ result[0];
        result.remove(0);
        result.push(0);
        for (r, &g) in result.iter_mut().zip(generator.iter()) {
            *r ^= gf_mul(g, factor);
        }
    }
    result
}

fn alignment_coords(version: usize) -> Vec<usize> {
    match version {
        1 => Vec::new(),
        2 => vec![6, 18],
        3 => vec![6, 22],
        4 => vec![6, 26],
        5 => vec![6, 30],
        6 => vec![6, 34],
        7 => vec![6, 22, 38],
        8 => vec![6, 24, 42],
        9 => vec![6, 26, 46],
        10 => vec![6, 28, 50],
        _ => Vec::new(),
    }
}

fn set_func(grid: &mut [bool], is_func: &mut [bool], size: usize, x: usize, y: usize, dark: bool) {
    if x < size && y < size {
        grid[y * size + x] = dark;
        is_func[y * size + x] = true;
    }
}

fn draw_finder_pattern(grid: &mut [bool], is_func: &mut [bool], size: usize, cx: isize, cy: isize) {
    for dy in -4..=4_isize {
        for dx in -4..=4_isize {
            let x = cx + dx;
            let y = cy + dy;
            if x >= 0
                && y >= 0
                && let (Ok(ux), Ok(uy)) = (usize::try_from(x), usize::try_from(y))
                && ux < size
                && uy < size
            {
                let dist = dx.abs().max(dy.abs());
                let dark = dist != 2 && dist != 4;
                set_func(grid, is_func, size, ux, uy, dark);
            }
        }
    }
}

fn draw_alignment_pattern(
    grid: &mut [bool],
    is_func: &mut [bool],
    size: usize,
    cx: usize,
    cy: usize,
) {
    for dy in -2..=2_isize {
        for dx in -2..=2_isize {
            let ux = usize::try_from(isize::try_from(cx).unwrap_or(0) + dx).unwrap_or(0);
            let uy = usize::try_from(isize::try_from(cy).unwrap_or(0) + dy).unwrap_or(0);
            let dist = dx.abs().max(dy.abs());
            set_func(grid, is_func, size, ux, uy, dist != 1);
        }
    }
}

fn draw_function_patterns(grid: &mut [bool], is_func: &mut [bool], size: usize, version: usize) {
    for i in 0..size {
        set_func(grid, is_func, size, 6, i, i % 2 == 0);
        set_func(grid, is_func, size, i, 6, i % 2 == 0);
    }

    let max_coord = isize::try_from(size).unwrap_or(21);
    draw_finder_pattern(grid, is_func, size, 3, 3);
    draw_finder_pattern(grid, is_func, size, max_coord - 4, 3);
    draw_finder_pattern(grid, is_func, size, 3, max_coord - 4);

    let coords = alignment_coords(version);
    let n = coords.len();
    for (i, &cy) in coords.iter().enumerate() {
        for (j, &cx) in coords.iter().enumerate() {
            if (i == 0 && j == 0) || (i == 0 && j == n - 1) || (i == n - 1 && j == 0) {
                continue;
            }
            draw_alignment_pattern(grid, is_func, size, cx, cy);
        }
    }

    // Reserve format information areas
    for i in 0..=8 {
        if i != 6 {
            set_func(grid, is_func, size, 8, i, false);
            set_func(grid, is_func, size, i, 8, false);
        }
    }
    for i in 0..8 {
        set_func(grid, is_func, size, size - 1 - i, 8, false);
        set_func(grid, is_func, size, 8, size - 1 - i, false);
    }
    // Always-dark module
    set_func(grid, is_func, size, 8, size - 8, true);
}

fn draw_codewords(grid: &mut [bool], is_func: &[bool], size: usize, codewords: &[u8]) {
    let mut bit_idx = 0_usize;
    let total_bits = codewords.len() * 8;

    let mut right = isize::try_from(size - 1).unwrap_or(20);
    while right >= 1 {
        if right == 6 {
            right -= 1;
        }
        let upward = ((right + 1) & 2) == 0;
        for vert in 0..size {
            let y = if upward { size - 1 - vert } else { vert };
            for j in 0..2_isize {
                let x = usize::try_from(right - j).unwrap_or(0);
                if !is_func[y * size + x] {
                    let dark = if bit_idx < total_bits {
                        let byte = codewords[bit_idx / 8];
                        let shift = 7 - (bit_idx % 8);
                        ((byte >> shift) & 1) != 0
                    } else {
                        false
                    };
                    grid[y * size + x] = dark;
                    bit_idx += 1;
                }
            }
        }
        right -= 2;
    }
}

fn apply_mask(grid: &mut [bool], is_func: &[bool], size: usize, mask: u8) {
    for y in 0..size {
        for x in 0..size {
            if is_func[y * size + x] {
                continue;
            }
            let invert = match mask {
                0 => (x + y) % 2 == 0,
                1 => y % 2 == 0,
                2 => x % 3 == 0,
                3 => (x + y) % 3 == 0,
                4 => (y / 2 + x / 3) % 2 == 0,
                5 => (x * y) % 2 + (x * y) % 3 == 0,
                6 => ((x * y) % 2 + (x * y) % 3) % 2 == 0,
                _ => ((x + y) % 2 + (x * y) % 3) % 2 == 0,
            };
            if invert {
                grid[y * size + x] = !grid[y * size + x];
            }
        }
    }
}

fn draw_format_bits(grid: &mut [bool], size: usize, mask: u8) {
    // Level L indicator is 01
    let data = (0b01_u16 << 3) | u16::from(mask);
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
    }
    let bits = ((data << 10) | rem) ^ 0x5412;

    for i in 0..=5 {
        grid[i * size + 8] = ((bits >> i) & 1) != 0;
    }
    grid[7 * size + 8] = ((bits >> 6) & 1) != 0;
    grid[8 * size + 8] = ((bits >> 7) & 1) != 0;
    grid[8 * size + 7] = ((bits >> 8) & 1) != 0;
    for i in 9..15 {
        grid[8 * size + (14 - i)] = ((bits >> i) & 1) != 0;
    }

    for i in 0..8 {
        grid[8 * size + (size - 1 - i)] = ((bits >> i) & 1) != 0;
    }
    for i in 8..15 {
        grid[(size - 15 + i) * size + 8] = ((bits >> i) & 1) != 0;
    }
    grid[(size - 8) * size + 8] = true;
}

fn compute_penalty(grid: &[bool], size: usize) -> u32 {
    let mut score = 0_u32;
    for y in 0..size {
        let mut run_color = grid[y * size];
        let mut run_len = 1_u32;
        for x in 1..size {
            let c = grid[y * size + x];
            if c == run_color {
                run_len += 1;
                if run_len == 5 {
                    score += 3;
                } else if run_len > 5 {
                    score += 1;
                }
            } else {
                run_color = c;
                run_len = 1;
            }
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_synthetic_pairing_payload_into_valid_qr_matrix() {
        let payload = r#"{"protocol_version":1,"host":"127.0.0.1","port":8765,"fingerprint_sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","pin":"654321"}"#;
        let qr = QrMatrix::encode_bytes(payload.as_bytes()).expect("encode pairing QR");
        assert!(qr.size() >= 41);
        // Top-left finder pattern corner and center must be dark
        assert!(qr.get(0, 0));
        assert!(qr.get(3, 3));
        assert!(!qr.get(1, 1));
    }
}
