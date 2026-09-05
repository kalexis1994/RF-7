//! The algorithm as a picture: carriers along the bottom, each modulator
//! stacked above what it modulates, the feedback path drawn as a loop.
//!
//! The layout is computed from the routing table, not drawn by hand, so the
//! picture cannot disagree with what the engine does. Every operator's level
//! is the length of the longest path from it down to the output; its column
//! is the mean of its targets' columns, nudged apart where two would meet.

use rf7_dsp::{ALGORITHMS, Algorithm};

const OPERATORS: usize = 6;
const COLUMN: f64 = 46.0;
const ROW: f64 = 40.0;
const BOX_W: f64 = 30.0;
const BOX_H: f64 = 22.0;
const MARGIN: f64 = 14.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// One (column, level) per operator; level 0 is the output row.
    pub positions: [(f64, usize); OPERATORS],
    pub columns: usize,
    pub levels: usize,
}

fn targets(algorithm: &Algorithm, operator: usize) -> Vec<usize> {
    (0..OPERATORS)
        .filter(|target| algorithm.modulators[*target] & (1 << operator) != 0)
        .collect()
}

pub fn layout(algorithm: &Algorithm) -> Layout {
    let mut level = [0usize; OPERATORS];
    // Every modulator has a higher index than what it modulates, so one
    // ascending pass settles the levels.
    for operator in 0..OPERATORS {
        if !algorithm.is_carrier(operator) {
            level[operator] = 1 + targets(algorithm, operator)
                .into_iter()
                .map(|t| level[t])
                .max()
                .unwrap_or(0);
        }
    }
    let levels = level.iter().max().copied().unwrap_or(0) + 1;
    let mut column = [0f64; OPERATORS];
    let mut next_carrier = 0.0;
    for (operator, slot) in column.iter_mut().enumerate() {
        if algorithm.is_carrier(operator) {
            *slot = next_carrier;
            next_carrier += 1.0;
        }
    }
    for current in 1..levels {
        let mut row: Vec<usize> = (0..OPERATORS).filter(|op| level[*op] == current).collect();
        for &operator in &row {
            let targets = targets(algorithm, operator);
            column[operator] =
                targets.iter().map(|t| column[*t]).sum::<f64>() / targets.len() as f64;
        }
        row.sort_by(|a, b| column[*a].total_cmp(&column[*b]).then(a.cmp(b)));
        // Two operators over the same target sit side by side, centred.
        let mut index = 0;
        while index < row.len() {
            let mut end = index + 1;
            while end < row.len() && (column[row[end]] - column[row[index]]).abs() < 0.5 {
                end += 1;
            }
            let group = end - index;
            let centre = column[row[index]];
            for (offset, &operator) in row[index..end].iter().enumerate() {
                column[operator] = centre + offset as f64 - (group as f64 - 1.0) / 2.0;
            }
            index = end;
        }
        for pair in 1..row.len() {
            let (left, right) = (row[pair - 1], row[pair]);
            if column[right] - column[left] < 1.0 {
                column[right] = column[left] + 1.0;
            }
        }
    }
    let minimum = column.iter().copied().fold(f64::INFINITY, f64::min);
    let mut positions = [(0.0, 0); OPERATORS];
    for operator in 0..OPERATORS {
        positions[operator] = (column[operator] - minimum, level[operator]);
    }
    let width = positions.iter().map(|(c, _)| *c).fold(0.0, f64::max);
    Layout {
        positions,
        columns: width.ceil() as usize + 1,
        levels,
    }
}

/// The SVG for one algorithm, panel-numbered 1..=32.
pub fn svg(number: usize) -> String {
    let algorithm = &ALGORITHMS[number.clamp(1, 32) - 1];
    let layout = layout(algorithm);
    let width = MARGIN * 2.0 + layout.columns as f64 * COLUMN;
    let height = MARGIN * 2.0 + layout.levels as f64 * ROW + 10.0;
    let centre = |operator: usize| -> (f64, f64) {
        let (column, level) = layout.positions[operator];
        (
            MARGIN + column * COLUMN + COLUMN / 2.0,
            height - MARGIN - 10.0 - level as f64 * ROW - ROW / 2.0,
        )
    };
    let mut out = format!(
        "<svg class=\"alg\" viewBox=\"0 0 {width:.0} {height:.0}\" role=\"img\" aria-label=\"Algorithm {number}\">"
    );
    // Output rail under the carriers.
    let rail_y = height - MARGIN - 4.0;
    out.push_str(&format!(
        "<line class=\"rail\" x1=\"{:.1}\" y1=\"{rail_y:.1}\" x2=\"{:.1}\" y2=\"{rail_y:.1}\"/>",
        MARGIN,
        width - MARGIN
    ));
    for operator in 0..OPERATORS {
        let (x, y) = centre(operator);
        if algorithm.is_carrier(operator) {
            out.push_str(&format!(
                "<line class=\"wire\" x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{x:.1}\" y2=\"{rail_y:.1}\"/>",
                y + BOX_H / 2.0
            ));
        }
        for target in targets(algorithm, operator) {
            let (tx, ty) = centre(target);
            out.push_str(&format!(
                "<line class=\"wire\" x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{tx:.1}\" y2=\"{:.1}\"/>",
                y + BOX_H / 2.0,
                ty - BOX_H / 2.0
            ));
        }
    }
    let (source, destination) = (
        usize::from(algorithm.feedback.0),
        usize::from(algorithm.feedback.1),
    );
    let (sx, sy) = centre(source);
    if source == destination {
        out.push_str(&format!(
            "<path class=\"loop\" d=\"M{:.1} {:.1} h9 v{:.1} h-9\"/>",
            sx + BOX_W / 2.0,
            sy + BOX_H / 4.0,
            -BOX_H / 2.0
        ));
    } else {
        let (dx, dy) = centre(destination);
        let right = sx.max(dx) + BOX_W / 2.0 + 9.0;
        out.push_str(&format!(
            "<path class=\"loop\" d=\"M{:.1} {:.1} H{right:.1} V{:.1} H{:.1}\"/>",
            sx + BOX_W / 2.0,
            sy,
            dy,
            dx + BOX_W / 2.0
        ));
    }
    for operator in 0..OPERATORS {
        let (x, y) = centre(operator);
        let class = if algorithm.is_carrier(operator) {
            "op carrier"
        } else {
            "op modulator"
        };
        out.push_str(&format!(
            "<rect class=\"{class}\" x=\"{:.1}\" y=\"{:.1}\" width=\"{BOX_W}\" height=\"{BOX_H}\" rx=\"4\"/><text x=\"{x:.1}\" y=\"{:.1}\">{}</text>",
            x - BOX_W / 2.0,
            y - BOX_H / 2.0,
            y + 4.5,
            operator + 1
        ));
    }
    out.push_str("</svg>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carriers_sit_on_the_output_row_and_modulators_above_their_targets() {
        for (index, algorithm) in ALGORITHMS.iter().enumerate() {
            let layout = layout(algorithm);
            for operator in 0..OPERATORS {
                let (column, level) = layout.positions[operator];
                assert!(
                    column >= 0.0 && column < layout.columns as f64,
                    "alg {}",
                    index + 1
                );
                if algorithm.is_carrier(operator) {
                    assert_eq!(level, 0, "alg {} op {}", index + 1, operator + 1);
                } else {
                    for target in targets(algorithm, operator) {
                        assert!(
                            level > layout.positions[target].1,
                            "alg {} op {}",
                            index + 1,
                            operator + 1
                        );
                    }
                }
            }
            // No two operators share a cell.
            for a in 0..OPERATORS {
                for b in a + 1..OPERATORS {
                    let (ca, la) = layout.positions[a];
                    let (cb, lb) = layout.positions[b];
                    assert!(
                        la != lb || (ca - cb).abs() >= 0.99,
                        "alg {} ops {} {}",
                        index + 1,
                        a + 1,
                        b + 1
                    );
                }
            }
        }
    }

    #[test]
    fn familiar_shapes_come_out_as_expected() {
        // Algorithm 32: six carriers in a row.
        let flat = layout(&ALGORITHMS[31]);
        assert_eq!(flat.levels, 1);
        assert_eq!(flat.columns, 6);
        // Algorithm 1: a two-stack and a four-stack.
        let one = layout(&ALGORITHMS[0]);
        assert_eq!(one.levels, 4);
        assert_eq!(one.columns, 2);
        assert_eq!(one.positions[5].1, 3, "OP6 tops the long chain");
        // Algorithm 7: OP4 and OP5 side by side over OP3.
        let seven = layout(&ALGORITHMS[6]);
        assert_eq!(seven.positions[3].1, 1);
        assert_eq!(seven.positions[4].1, 1);
        assert!((seven.positions[3].0 - seven.positions[4].0).abs() >= 0.99);
    }

    #[test]
    fn every_algorithm_draws_six_boxes_and_one_loop() {
        for number in 1..=32 {
            let svg = svg(number);
            assert!(svg.starts_with("<svg"), "{number}");
            assert_eq!(svg.matches("<rect class=\"op").count(), 6, "{number}");
            assert_eq!(svg.matches("class=\"loop\"").count(), 1, "{number}");
            assert_eq!(
                svg.matches("class=\"op carrier\"").count(),
                ALGORITHMS[number - 1].carriers.count_ones() as usize
            );
        }
    }
}
