use std::fmt::Write;

use ctdesign::{
    MENUBAR_BATTERY_BODY_HEIGHT, MENUBAR_BATTERY_BODY_WIDTH, MENUBAR_BATTERY_BOLT_GAP,
    MENUBAR_BATTERY_DIGIT_HEIGHT, MENUBAR_BATTERY_DIGIT_PITCH, MENUBAR_BATTERY_HUNDRED_GAP,
    MENUBAR_BATTERY_HUNDRED_HEIGHT, MENUBAR_BATTERY_LEVEL_MIN, MENUBAR_BATTERY_TRACK, Rgba,
};

const BODY: &str = "M4 0h16a4 4 0 0 1 4 4v4a4 4 0 0 1-4 4H4a4 4 0 0 1-4-4V4a4 4 0 0 1 4-4z";
const NUB: &str = "M25 4h.6a1.4 1.4 0 0 1 1.4 1.4v1.2a1.4 1.4 0 0 1-1.4 1.4H25z";

struct Glyph {
    width: f32,
    height: f32,
    gap: f32,
    x0: f32,
    y0: f32,
    rest: &'static str,
}

const fn digit(x0: f32, y0: f32, rest: &'static str) -> Glyph {
    Glyph {
        width: MENUBAR_BATTERY_DIGIT_PITCH,
        height: MENUBAR_BATTERY_DIGIT_HEIGHT,
        gap: 0.0,
        x0,
        y0,
        rest,
    }
}

const fn hundred(width: f32, x0: f32, y0: f32, rest: &'static str) -> Glyph {
    Glyph {
        width,
        height: MENUBAR_BATTERY_HUNDRED_HEIGHT,
        gap: MENUBAR_BATTERY_HUNDRED_GAP,
        x0,
        y0,
        rest,
    }
}

static BOLT: Glyph = Glyph {
    width: 5.15,
    height: 8.16,
    gap: MENUBAR_BATTERY_BOLT_GAP,
    x0: 3.27,
    y0: 0.14,
    rest: "q.1-.14 .26-.1q.25 .05 .19 .3l-.63 2.79h1.79q.08 0 .14 .05q.1 .07 .12 .16q.01 .08-.06 .19l-3.2 4.5q-.1 .13-.26 .09q-.12-.02-.17-.1q-.05-.07-.02-.19l.63-2.8h-1.78q-.08 0-.15-.05q-.1-.07-.12-.16q-.01-.08 .06-.18z",
};

static HUNDRED_ONE: Glyph = hundred(
    2.63,
    2.63,
    -0.09,
    "v6.25h-1.13v-5.15h-.04l-1.46 .94v-1.04l1.55-1z",
);

static HUNDRED_ZERO: Glyph = hundred(
    4.82,
    2.4,
    6.29,
    "q-.76 0-1.3-.38q-.54-.38-.82-1.11q-.29-.72-.29-1.75q0-1.02 .29-1.74q.29-.72 .83-1.1q.54-.38 1.29-.38q.75 0 1.29 .38q.54 .38 .83 1.1q.29 .72 .29 1.74q0 1.03-.29 1.75q-.29 .73-.83 1.11q-.54 .38-1.29 .38zm0-.95q.58 0 .92-.58q.34-.58 .34-1.71q0-.74-.15-1.25q-.16-.51-.44-.76q-.28-.26-.67-.26q-.59 0-.92 .58q-.34 .58-.35 1.69q0 .75 .15 1.26q.16 .51 .44 .77q.29 .26 .68 .26z",
);

static DIGITS: [Glyph; 10] = [
    digit(
        2.94,
        7.3,
        "q-.88 0-1.51-.44q-.62-.45-.96-1.29q-.33-.85-.33-2.04q0-1.19 .33-2.02q.34-.84 .97-1.28q.62-.44 1.5-.44q.87 0 1.5 .44q.63 .44 .96 1.28q.34 .84 .34 2.02q0 1.2-.34 2.04q-.33 .84-.96 1.29q-.63 .44-1.5 .44zm0-1.11q.68 0 1.08-.67q.39-.68 .39-1.99q0-.86-.18-1.45q-.18-.59-.51-.89q-.33-.3-.78-.3q-.68 0-1.07 .67q-.4 .67-.4 1.97q-.01 .87 .17 1.47q.18 .59 .51 .89q.33 .3 .79 .3z",
    ),
    digit(
        4.48,
        -0.11,
        "v7.27h-1.32v-5.99h-.04l-1.7 1.09v-1.21l1.8-1.16z",
    ),
    digit(
        0.54,
        7.16,
        "v-.95l2.52-2.47q.37-.37 .61-.65q.24-.29 .36-.55q.12-.27 .12-.57q0-.35-.16-.59q-.15-.25-.43-.39q-.27-.13-.62-.13q-.36 0-.62 .15q-.27 .14-.42 .41q-.15 .27-.15 .65h-1.25q0-.7 .31-1.21q.32-.51 .87-.79q.56-.28 1.28-.28q.73 0 1.29 .27q.55 .28 .86 .75q.31 .48 .31 1.09q0 .41-.16 .8q-.16 .4-.55 .88q-.39 .48-1.1 1.16l-1.25 1.27v.05h3.17v1.1z",
    ),
    digit(
        2.98,
        7.26,
        "q-.77 0-1.36-.26q-.6-.26-.94-.73q-.35-.47-.37-1.09h1.34q.02 .3 .19 .52q.18 .22 .48 .34q.29 .12 .66 .12q.39 0 .69-.14q.3-.14 .47-.38q.17-.25 .17-.57q0-.33-.17-.58q-.18-.25-.51-.39q-.32-.14-.79-.14h-.64v-1.02h.64q.38 0 .67-.13q.29-.13 .45-.37q.16-.24 .16-.56q0-.31-.14-.54q-.14-.22-.39-.35q-.26-.13-.6-.13q-.33 0-.61 .12q-.29 .12-.46 .34q-.18 .22-.19 .53h-1.26q.01-.61 .35-1.07q.34-.47 .91-.73q.56-.26 1.27-.26q.73 0 1.27 .27q.53 .27 .83 .73q.29 .46 .29 1.01q.01 .62-.36 1.03q-.36 .42-.95 .55v.05q.77 .11 1.18 .57q.41 .46 .4 1.14q0 .61-.34 1.09q-.35 .49-.95 .76q-.61 .27-1.39 .27z",
    ),
    digit(
        0.09,
        5.81,
        "v-1.04l3.08-4.88h1.64v4.84h.93v1.08h-.93v1.35h-1.25v-1.35zm3.48-1.08v-3.35h-.06l-2.07 3.29v.06z",
    ),
    digit(
        3.02,
        7.26,
        "q-.71 0-1.27-.27q-.56-.26-.89-.73q-.33-.47-.35-1.08h1.28q.03 .45 .38 .73q.35 .29 .85 .29q.38 0 .69-.18q.3-.18 .47-.49q.18-.32 .17-.73q.01-.41-.17-.73q-.18-.32-.49-.5q-.31-.18-.71-.18q-.32-.01-.64 .12q-.32 .12-.5 .33l-1.19-.2l.38-3.75h4.22v1.1h-3.13l-.21 1.93h.04q.2-.24 .57-.4q.37-.15 .81-.15q.66 0 1.18 .31q.52 .31 .82 .85q.3 .54 .3 1.24q0 .72-.33 1.29q-.34 .56-.92 .88q-.59 .32-1.36 .32z",
    ),
    digit(
        3.01,
        7.26,
        "q-.52 0-1.02-.18q-.49-.18-.89-.59q-.4-.4-.63-1.08q-.24-.68-.23-1.68q0-.93 .19-1.66q.2-.73 .58-1.24q.37-.51 .9-.77q.52-.27 1.18-.27q.68 0 1.21 .27q.53 .27 .86 .74q.33 .46 .41 1.05h-1.3q-.1-.42-.41-.67q-.3-.25-.77-.25q-.76 0-1.17 .66q-.4 .65-.41 1.8h.05q.18-.31 .45-.53q.28-.23 .63-.35q.35-.12 .74-.12q.64 0 1.15 .3q.51 .31 .81 .84q.3 .54 .29 1.23q.01 .71-.32 1.28q-.33 .57-.92 .9q-.59 .33-1.38 .32zm0-1.06q.38 0 .69-.19q.3-.19 .48-.51q.18-.32 .18-.72q0-.39-.17-.7q-.18-.32-.48-.5q-.3-.19-.69-.19q-.28 0-.53 .11q-.25 .11-.44 .31q-.18 .19-.29 .44q-.1 .26-.1 .54q0 .38 .17 .7q.18 .32 .48 .52q.31 .19 .7 .19z",
    ),
    digit(
        0.85,
        7.16,
        "l3.09-6.12v-.05h-3.59v-1.1h4.95v1.13l-3.09 6.14z",
    ),
    digit(
        2.93,
        7.26,
        "q-.79 0-1.4-.26q-.61-.27-.96-.73q-.34-.47-.34-1.06q0-.45 .2-.84q.2-.38 .55-.64q.34-.26 .77-.33v-.05q-.56-.12-.91-.57q-.34-.45-.34-1.04q0-.56 .31-1q.32-.44 .87-.7q.55-.25 1.25-.25q.7 0 1.25 .25q.55 .26 .87 .7q.32 .44 .32 1q0 .59-.35 1.04q-.35 .45-.9 .57v.05q.42 .07 .77 .33q.34 .26 .54 .64q.21 .39 .21 .84q0 .59-.35 1.06q-.35 .46-.96 .73q-.61 .26-1.4 .26zm0-1.01q.41 0 .71-.14q.31-.14 .47-.4q.17-.26 .17-.59q0-.35-.18-.62q-.17-.26-.47-.42q-.31-.15-.7-.15q-.39 0-.69 .15q-.31 .16-.48 .42q-.18 .27-.17 .62q-.01 .33 .16 .59q.17 .25 .47 .39q.3 .15 .71 .15zm0-3.32q.34 0 .6-.13q.25-.14 .4-.38q.15-.24 .16-.56q-.01-.31-.15-.54q-.15-.24-.41-.37q-.26-.13-.6-.13q-.34 0-.6 .13q-.26 .13-.41 .37q-.14 .23-.14 .54q0 .32 .15 .56q.15 .24 .41 .38q.26 .13 .59 .13z",
    ),
    digit(
        2.87,
        -0.21,
        "q.53 0 1.02 .18q.5 .18 .89 .59q.4 .4 .64 1.07q.23 .68 .23 1.67q0 .94-.2 1.68q-.2 .73-.57 1.24q-.37 .51-.9 .78q-.52 .26-1.18 .26q-.69 0-1.22-.27q-.53-.27-.86-.74q-.32-.47-.4-1.07h1.3q.1 .43 .4 .69q.31 .25 .78 .25q.76 0 1.16-.65q.41-.66 .41-1.83h-.05q-.17 .31-.45 .54q-.27 .22-.62 .34q-.35 .12-.75 .12q-.63 0-1.14-.3q-.51-.31-.81-.84q-.3-.53-.3-1.22q0-.71 .33-1.27q.33-.57 .92-.9q.59-.33 1.37-.32zm.01 1.07q-.39 0-.69 .18q-.31 .19-.49 .51q-.17 .31-.17 .71q0 .38 .17 .7q.18 .31 .48 .5q.3 .18 .68 .18q.29 0 .54-.11q.25-.11 .43-.3q.19-.2 .3-.45q.1-.26 .1-.54q0-.37-.18-.68q-.17-.32-.48-.51q-.3-.19-.69-.19z",
    ),
];

pub(super) fn svg(
    percent: f64,
    ink: Rgba,
    fill: Rgba,
    charging: bool,
    show_percentage: bool,
) -> String {
    let percent = percent.round().clamp(0.0, 100.0) as u8;
    let track = rgba(Rgba {
        a: ink.a * MENUBAR_BATTERY_TRACK,
        ..ink
    });
    let fill = rgba(fill);
    let level =
        (f32::from(percent) / 100.0 * MENUBAR_BATTERY_BODY_WIDTH).max(MENUBAR_BATTERY_LEVEL_MIN);
    let body = format!("{BODY}{}", knockout(percent, charging, show_percentage));
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 27 12"><defs><clipPath id="level"><rect width="{level:.2}" height="12"/></clipPath><clipPath id="rest"><rect x="{level:.2}" width="27" height="12"/></clipPath></defs><path d="{body}" fill-rule="evenodd" fill="{fill}" clip-path="url(#level)"/><path d="{body}" fill-rule="evenodd" fill="{track}" clip-path="url(#rest)"/><path d="{NUB}" fill="{track}"/></svg>"#
    )
}

fn glyphs(percent: u8, charging: bool, show_percentage: bool) -> ([&'static Glyph; 4], usize) {
    let mut glyphs = [&BOLT; 4];
    let count = match (show_percentage, percent) {
        (false, _) => 0,
        (true, 0..=9) => {
            glyphs[0] = &DIGITS[usize::from(percent)];
            1
        }
        (true, 10..=99) => {
            glyphs[0] = &DIGITS[usize::from(percent / 10)];
            glyphs[1] = &DIGITS[usize::from(percent % 10)];
            2
        }
        (true, 100..) => {
            glyphs[..3].copy_from_slice(&[&HUNDRED_ONE, &HUNDRED_ZERO, &HUNDRED_ZERO]);
            3
        }
    };
    let bolt = charging && count < 3;
    (glyphs, count + usize::from(bolt))
}

fn knockout(percent: u8, charging: bool, show_percentage: bool) -> String {
    let (glyphs, count) = glyphs(percent, charging, show_percentage);
    let glyphs = &glyphs[..count];
    let mut x = (MENUBAR_BATTERY_BODY_WIDTH - group_width(glyphs)) / 2.0;
    let mut path = String::new();
    for (index, glyph) in glyphs.iter().enumerate() {
        if index > 0 {
            x += glyph.gap;
        }
        let y = (MENUBAR_BATTERY_BODY_HEIGHT - glyph.height) / 2.0;
        write!(
            path,
            "M{:.2} {:.2}{}",
            x + glyph.x0,
            y + glyph.y0,
            glyph.rest
        )
        .ok();
        x += glyph.width;
    }
    path
}

fn group_width(glyphs: &[&Glyph]) -> f32 {
    let widths: f32 = glyphs.iter().map(|glyph| glyph.width).sum();
    let gaps: f32 = glyphs.iter().skip(1).map(|glyph| glyph.gap).sum();
    widths + gaps
}

fn rgba(color: Rgba) -> String {
    let [r, g, b, a] = color.to_rgba8();
    format!("rgba({r},{g},{b},{:.3})", f32::from(a) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARGIN_MIN: f32 = 2.0;

    #[test]
    fn every_percentage_leaves_a_margin_inside_the_body_with_and_without_the_bolt() {
        for percent in 0..=100 {
            for charging in [false, true] {
                let (glyphs, count) = glyphs(percent, charging, true);
                let width = group_width(&glyphs[..count]);
                assert!(
                    width <= MENUBAR_BATTERY_BODY_WIDTH - 2.0 * MARGIN_MIN,
                    "{percent} {charging} {width}"
                );
            }
        }
    }

    #[test]
    fn hiding_the_percentage_leaves_only_the_bolt_while_charging() {
        assert!(knockout(80, false, false).is_empty());
        assert_eq!(knockout(80, true, false).matches('M').count(), 1);
    }

    #[test]
    fn a_full_battery_drops_the_bolt_to_fit_three_digits() {
        assert_eq!(knockout(100, true, true), knockout(100, false, true));
    }
}
