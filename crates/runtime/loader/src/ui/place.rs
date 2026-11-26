//! Where a mod's panel sits beside one of the game's screens: an edge, a
//! place along it and an offset, as the game's layout takes them.

use enshwick_core::abi::UiExtension;

use crate::ui::listing::SCREEN_HEIGHT;

/// The width the game lays its interface out in, whatever the window's
/// resolution, the largest offset a place takes.
const LAYOUT_WIDTH: f32 = UiExtension::MAX_OFFSET;

/// A panel's place: its edge, where along it, how far it moves along the
/// edge and how far away from it, in layout units. Every value is checked by
/// [`Place::of`], so two places compare by value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Place {
    edge: u32,
    align: u32,
    shift: f32,
    inset: f32,
}

impl Place {
    /// The place at `edge`, centered along it.
    #[cfg(test)]
    pub(crate) const fn edge(edge: u32) -> Self {
        Self {
            edge,
            align: UiExtension::CENTER,
            shift: 0.0,
            inset: 0.0,
        }
    }

    /// The place `spec` asks for; `None` unless
    /// [`UiExtension::placement_holds`].
    pub(crate) fn of(spec: &UiExtension) -> Option<Self> {
        spec.placement_holds().then_some(Self {
            edge: spec.edge,
            align: spec.align,
            shift: spec.shift,
            inset: spec.inset,
        })
    }

    /// Whether the panel sits at the left or right edge, so its place along
    /// the edge is vertical.
    const fn side(self) -> bool {
        matches!(self.edge, UiExtension::LEFT | UiExtension::RIGHT)
    }

    /// The game's alignment word for the panel in the full-screen frame
    /// around it: the vertical part in the high byte, the horizontal in the
    /// low, 1 for the start, 2 the centre, 3 the end, as the HUD's columns
    /// take it.
    pub(crate) const fn align_word(self) -> u16 {
        let along: u16 = match self.align {
            UiExtension::START => 1,
            UiExtension::END => 3,
            _ => 2,
        };
        match self.edge {
            UiExtension::LEFT => along << 8 | 1,
            UiExtension::RIGHT => along << 8 | 3,
            UiExtension::TOP => 0x100 | along,
            _ => 0x300 | along,
        }
    }

    /// The padding (left, top, right, bottom) of the full-screen frame
    /// around the panel, `margin` on each side: the inset adds to the edge's
    /// side, and the shift moves the panel along the edge. A panel at the
    /// start moves by its start's padding, one at the end by its end's, and
    /// a centred one by half their difference, which stays twice the shift.
    /// The panel never passes the screen's border: no padding goes below
    /// zero, and the room left between two opposite paddings stays
    /// `width` across and zero high, the panel's height being its
    /// content's.
    pub(crate) fn padding(self, margin: f32, width: f32) -> [f32; 4] {
        let mut padding = self.unbounded(margin);
        for (near, far, extent) in [(0, 2, LAYOUT_WIDTH - width), (1, 3, SCREEN_HEIGHT)] {
            let over = padding[near] + padding[far] - extent;
            if over > 0.0 {
                let wide = if padding[near] >= padding[far] {
                    near
                } else {
                    far
                };
                padding[wide] = (padding[wide] - over).max(0.0);
            }
        }
        padding
    }

    /// [`Place::padding`] before the screen's far border holds it.
    fn unbounded(self, margin: f32) -> [f32; 4] {
        let mut padding = [margin; 4];
        let edge = match self.edge {
            UiExtension::LEFT => 0,
            UiExtension::TOP => 1,
            UiExtension::RIGHT => 2,
            _ => 3,
        };
        padding[edge] += self.inset;
        let (start, end) = if self.side() { (1, 3) } else { (0, 2) };
        let shift = self.shift;
        match self.align {
            UiExtension::START => padding[start] = (margin + shift).max(0.0),
            UiExtension::END => padding[end] = (margin - shift).max(0.0),
            _ => {
                let room = (shift.abs() - margin).max(0.0);
                padding[start] = margin + shift + room;
                padding[end] = margin - shift + room;
            }
        }
        padding
    }

    /// The order of panels' windows: edge by edge, as the loader drew them
    /// before places had more than an edge.
    #[cfg(windows)]
    pub(crate) const fn rank(self) -> u32 {
        self.edge
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(edge: u32, align: u32, shift: f32, inset: f32) -> UiExtension {
        enshwick_abi::headed!(UiExtension {
            edge,
            align,
            shift,
            inset,
            ..bytemuck::Zeroable::zeroed()
        })
    }

    #[test]
    fn a_spec_of_an_older_minor_reads_as_its_edge_centred() {
        let old = spec(UiExtension::RIGHT, 0, 0.0, 0.0);
        assert_eq!(Place::of(&old), Some(Place::edge(UiExtension::RIGHT)));
    }

    #[test]
    fn an_edge_an_alignment_or_an_offset_the_loader_lacks_is_refused() {
        let max = UiExtension::MAX_OFFSET;
        let refused = [
            spec(UiExtension::BOTTOM + 1, 0, 0.0, 0.0),
            spec(UiExtension::LEFT, UiExtension::END + 1, 0.0, 0.0),
            spec(UiExtension::LEFT, 0, f32::NAN, 0.0),
            spec(UiExtension::LEFT, 0, f32::INFINITY, 0.0),
            spec(UiExtension::LEFT, 0, max + 1.0, 0.0),
            spec(UiExtension::LEFT, 0, -max - 1.0, 0.0),
            spec(UiExtension::LEFT, 0, 0.0, -1.0),
            spec(UiExtension::LEFT, 0, 0.0, max + 1.0),
            spec(UiExtension::LEFT, 0, 0.0, f32::NAN),
            enshwick_abi::headed!(UiExtension {
                reserved_place: 1,
                ..spec(UiExtension::LEFT, 0, 0.0, 0.0)
            }),
        ];
        for spec in refused {
            assert_eq!(Place::of(&spec), None, "{spec:?}");
        }
        assert!(Place::of(&spec(UiExtension::BOTTOM, UiExtension::END, -max, max)).is_some());
    }

    #[test]
    fn each_edge_and_alignment_gives_the_game_s_word() {
        let word = |edge, align| Place::of(&spec(edge, align, 0.0, 0.0)).map(Place::align_word);
        let (centre, start, end) = (UiExtension::CENTER, UiExtension::START, UiExtension::END);
        assert_eq!(
            [
                UiExtension::LEFT,
                UiExtension::RIGHT,
                UiExtension::TOP,
                UiExtension::BOTTOM
            ]
            .map(|edge| word(edge, centre)),
            [Some(0x201), Some(0x203), Some(0x102), Some(0x302)],
            "centred, as before places had more than an edge"
        );
        assert_eq!(word(UiExtension::LEFT, start), Some(0x101), "top left");
        assert_eq!(word(UiExtension::RIGHT, end), Some(0x303), "bottom right");
        assert_eq!(word(UiExtension::TOP, end), Some(0x103), "top right");
        assert_eq!(word(UiExtension::BOTTOM, start), Some(0x301), "bottom left");
    }

    fn padding(edge: u32, align: u32, shift: f32, inset: f32) -> Option<[f32; 4]> {
        Place::of(&spec(edge, align, shift, inset)).map(|p| p.padding(48.0, 480.0))
    }

    #[test]
    fn a_shift_or_inset_toward_the_far_border_stops_there() {
        let (start, end, centre) = (UiExtension::START, UiExtension::END, UiExtension::CENTER);
        assert_eq!(
            padding(UiExtension::LEFT, start, 1500.0, 0.0),
            Some([48.0, 1032.0, 48.0, 48.0]),
            "down a side edge: the panel's top at most at the bottom margin"
        );
        assert_eq!(
            padding(UiExtension::LEFT, centre, -1000.0, 0.0),
            Some([48.0, 0.0, 48.0, 1080.0]),
            "up a side edge, centred"
        );
        assert_eq!(
            padding(UiExtension::TOP, end, -1900.0, 1900.0),
            Some([48.0, 1032.0, 1392.0, 48.0]),
            "left along the top and far in: the whole width still fits"
        );
        for edge in [
            UiExtension::LEFT,
            UiExtension::RIGHT,
            UiExtension::TOP,
            UiExtension::BOTTOM,
        ] {
            for align in [start, end, centre] {
                for shift in [-1920.0, -700.0, 0.0, 700.0, 1920.0] {
                    for inset in [0.0, 900.0, 1920.0] {
                        let [left, top, right, bottom] =
                            padding(edge, align, shift, inset).unwrap();
                        assert!([left, top, right, bottom].iter().all(|p| *p >= 0.0));
                        assert!(
                            left + right + 480.0 <= 1920.0 + 1e-3,
                            "{edge} {align} {shift}"
                        );
                        assert!(top + bottom <= 1080.0 + 1e-3, "{edge} {align} {shift}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_inset_pads_the_edge_s_side_and_a_panel_at_the_start_or_end_moves_by_the_shift() {
        let (start, end) = (UiExtension::START, UiExtension::END);
        assert_eq!(padding(UiExtension::LEFT, start, 0.0, 0.0), Some([48.0; 4]));
        assert_eq!(
            padding(UiExtension::LEFT, start, 362.0, 125.0),
            Some([173.0, 410.0, 48.0, 48.0]),
            "under the backpack grid at 16:9"
        );
        assert_eq!(
            padding(UiExtension::BOTTOM, start, -100.0, 20.0),
            Some([0.0, 48.0, 48.0, 68.0]),
            "held at the screen's border"
        );
        assert_eq!(
            padding(UiExtension::RIGHT, end, -20.0, 10.0),
            Some([48.0, 48.0, 58.0, 68.0])
        );
    }

    #[test]
    fn a_centred_panel_moves_by_the_whole_shift_even_past_the_margin() {
        let centre = UiExtension::CENTER;
        for shift in [0.0, 20.0, -20.0, 100.0, -300.0] {
            let [_, top, _, bottom] = padding(UiExtension::LEFT, centre, shift, 0.0).unwrap();
            assert!(top >= 0.0 && bottom >= 0.0, "shift {shift}");
            assert!((top - bottom - 2.0 * shift).abs() < 1e-3, "shift {shift}");
        }
        assert_eq!(
            padding(UiExtension::TOP, centre, 20.0, 0.0),
            Some([68.0, 48.0, 28.0, 48.0])
        );
    }
}
