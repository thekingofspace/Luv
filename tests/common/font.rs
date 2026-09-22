const UNITS_PER_EM: u16 = 1000;
const ASCENDER: i16 = 800;
const DESCENDER: i16 = -200;
const BLOCK_ADVANCE: u16 = 600;
const BLOCK: [(i16, i16); 4] = [(100, 0), (100, 700), (500, 700), (500, 0)];
const LETTERS: u16 = 26;

fn u16s(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_be_bytes()).collect()
}

fn block_glyph() -> Vec<u8> {
    let mut glyph = Vec::new();
    glyph.extend(1i16.to_be_bytes());
    for value in [100i16, 0, 500, 700] {
        glyph.extend(value.to_be_bytes());
    }
    glyph.extend(3u16.to_be_bytes());
    glyph.extend(0u16.to_be_bytes());
    glyph.extend([1u8; 4]);
    let mut previous = (0i16, 0i16);
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (x, y) in BLOCK {
        xs.extend((x - previous.0).to_be_bytes());
        ys.extend((y - previous.1).to_be_bytes());
        previous = (x, y);
    }
    glyph.extend(xs);
    glyph.extend(ys);
    while !glyph.len().is_multiple_of(4) {
        glyph.push(0);
    }
    glyph
}

fn cmap() -> Vec<u8> {
    let segments: [(u16, u16, u16); 3] = [
        (0x20, 0x20, 1u16.wrapping_sub(0x20)),
        (0x41, 0x5A, 2u16.wrapping_sub(0x41)),
        (0xFFFF, 0xFFFF, 1),
    ];
    let count = segments.len() as u16;
    let mut table = u16s(&[4, 0, 0, count * 2, 4, 1, 2]);
    table.extend(u16s(&segments.map(|segment| segment.1)));
    table.extend(u16s(&[0]));
    table.extend(u16s(&segments.map(|segment| segment.0)));
    table.extend(u16s(&segments.map(|segment| segment.2)));
    table.extend(u16s(&[0, 0, 0]));
    let length = table.len() as u16;
    table[2..4].copy_from_slice(&length.to_be_bytes());
    let mut cmap = u16s(&[0, 1, 3, 1]);
    cmap.extend(12u32.to_be_bytes());
    cmap.extend(table);
    cmap
}

pub fn block_font() -> Vec<u8> {
    let glyphs = 2 + LETTERS;
    let block = block_glyph();
    let mut glyf = Vec::new();
    let mut loca = vec![0u32, 0, 0];
    for _ in 0..LETTERS {
        glyf.extend(&block);
        loca.push(glyf.len() as u32);
    }
    let loca: Vec<u8> = loca.iter().flat_map(|offset| offset.to_be_bytes()).collect();

    let mut head = Vec::new();
    head.extend(0x0001_0000u32.to_be_bytes());
    head.extend(0x0001_0000u32.to_be_bytes());
    head.extend(0u32.to_be_bytes());
    head.extend(0x5F0F_3CF5u32.to_be_bytes());
    head.extend(u16s(&[0, UNITS_PER_EM]));
    head.extend([0u8; 16]);
    for value in [0i16, 0, 500, 700] {
        head.extend(value.to_be_bytes());
    }
    head.extend(u16s(&[0, 8]));
    head.extend(2i16.to_be_bytes());
    head.extend(1i16.to_be_bytes());
    head.extend(0i16.to_be_bytes());

    let mut hhea = Vec::new();
    hhea.extend(0x0001_0000u32.to_be_bytes());
    for value in [ASCENDER, DESCENDER, 0] {
        hhea.extend(value.to_be_bytes());
    }
    hhea.extend(BLOCK_ADVANCE.to_be_bytes());
    for value in [0i16, 0, 500, 1, 0, 0, 0, 0, 0, 0, 0] {
        hhea.extend(value.to_be_bytes());
    }
    hhea.extend(glyphs.to_be_bytes());

    let mut hmtx = Vec::new();
    for glyph in 0..glyphs {
        let (advance, bearing) = if glyph < 2 { (500u16, 0i16) } else { (BLOCK_ADVANCE, 100) };
        hmtx.extend(advance.to_be_bytes());
        hmtx.extend(bearing.to_be_bytes());
    }

    let mut maxp = Vec::new();
    maxp.extend(0x0001_0000u32.to_be_bytes());
    maxp.extend(u16s(&[glyphs, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]));

    let mut post = Vec::new();
    post.extend(0x0003_0000u32.to_be_bytes());
    post.extend(0u32.to_be_bytes());
    post.extend((-100i16).to_be_bytes());
    post.extend(50i16.to_be_bytes());
    post.extend([0u8; 20]);

    let tables: [(&[u8; 4], Vec<u8>); 8] = [
        (b"cmap", cmap()),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
        (b"post", post),
    ];
    let count = tables.len() as u16;
    let mut font = Vec::new();
    font.extend(0x0001_0000u32.to_be_bytes());
    font.extend(u16s(&[count, 128, 3, count * 16 - 128]));
    let mut offset = 12 + 16 * tables.len();
    let mut data = Vec::new();
    for (tag, table) in &tables {
        font.extend(*tag);
        font.extend(0u32.to_be_bytes());
        font.extend((offset as u32).to_be_bytes());
        font.extend((table.len() as u32).to_be_bytes());
        let mut padded = table.clone();
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        offset += padded.len();
        data.extend(padded);
    }
    font.extend(data);
    font
}
