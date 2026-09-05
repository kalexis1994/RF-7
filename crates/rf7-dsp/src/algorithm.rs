//! The 32 operator routings.
//!
//! Each algorithm is stated as three facts: which operators reach the output,
//! which operators modulate each operator, and where the one feedback path
//! runs. Diagrams are the usual way to publish this and the worst way to check
//! it, so the invariants a diagram makes obvious are asserted in the tests
//! below instead.
//!
//! Every modulator has a higher number than the operator it modulates, so one
//! descending pass from OP6 to OP1 evaluates any algorithm. The feedback path
//! is the sole exception and reads the previous sample, which is what makes it
//! feedback.

use crate::OPERATORS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Algorithm {
    /// Bit `i` set means OP(i+1) is summed into the output.
    pub carriers: u8,
    /// Bit `j` of `modulators[i]` set means OP(j+1) modulates OP(i+1).
    pub modulators: [u8; OPERATORS],
    /// The feedback path, zero-based: the source operator's output modulates
    /// the destination operator. Equal indexes are the usual self-loop;
    /// algorithms 4 and 6 close the loop around a chain instead.
    pub feedback: (u8, u8),
}

impl Algorithm {
    pub const fn is_carrier(&self, operator: usize) -> bool {
        self.carriers & (1 << operator) != 0
    }
}

/// Indexed 0..=31 for the panel's algorithms 1..=32.
pub const ALGORITHMS: [Algorithm; 32] = [
    // 1: 2->1, 6->5->4->3
    algorithm(0b00_0101, [0x02, 0, 0x08, 0x10, 0x20, 0], (5, 5)),
    // 2: as 1, feedback on OP2
    algorithm(0b00_0101, [0x02, 0, 0x08, 0x10, 0x20, 0], (1, 1)),
    // 3: 3->2->1, 6->5->4
    algorithm(0b00_1001, [0x02, 0x04, 0, 0x10, 0x20, 0], (5, 5)),
    // 4: as 3, with the loop closed around OP6..OP4
    algorithm(0b00_1001, [0x02, 0x04, 0, 0x10, 0x20, 0], (3, 5)),
    // 5: 2->1, 4->3, 6->5
    algorithm(0b01_0101, [0x02, 0, 0x08, 0, 0x20, 0], (5, 5)),
    // 6: as 5, with the loop closed around OP6..OP5
    algorithm(0b01_0101, [0x02, 0, 0x08, 0, 0x20, 0], (4, 5)),
    // 7: 2->1, (4, 6->5)->3
    algorithm(0b00_0101, [0x02, 0, 0x18, 0, 0x20, 0], (5, 5)),
    // 8: as 7, feedback on OP4
    algorithm(0b00_0101, [0x02, 0, 0x18, 0, 0x20, 0], (3, 3)),
    // 9: as 7, feedback on OP2
    algorithm(0b00_0101, [0x02, 0, 0x18, 0, 0x20, 0], (1, 1)),
    // 10: (5, 6)->4, 3->2->1
    algorithm(0b00_1001, [0x02, 0x04, 0, 0x30, 0, 0], (2, 2)),
    // 11: as 10, feedback on OP6
    algorithm(0b00_1001, [0x02, 0x04, 0, 0x30, 0, 0], (5, 5)),
    // 12: (4, 5, 6)->3, 2->1
    algorithm(0b00_0101, [0x02, 0, 0x38, 0, 0, 0], (1, 1)),
    // 13: as 12, feedback on OP6
    algorithm(0b00_0101, [0x02, 0, 0x38, 0, 0, 0], (5, 5)),
    // 14: 2->1, (5, 6)->4->3
    algorithm(0b00_0101, [0x02, 0, 0x08, 0x30, 0, 0], (5, 5)),
    // 15: as 14, feedback on OP2
    algorithm(0b00_0101, [0x02, 0, 0x08, 0x30, 0, 0], (1, 1)),
    // 16: (2, 4->3, 6->5)->1
    algorithm(0b00_0001, [0x16, 0, 0x08, 0, 0x20, 0], (5, 5)),
    // 17: as 16, feedback on OP2
    algorithm(0b00_0001, [0x16, 0, 0x08, 0, 0x20, 0], (1, 1)),
    // 18: (2, 3, 6->5->4)->1
    algorithm(0b00_0001, [0x0e, 0, 0, 0x10, 0x20, 0], (2, 2)),
    // 19: 3->2->1, 6->(4, 5)
    algorithm(0b01_1001, [0x02, 0x04, 0, 0x20, 0x20, 0], (5, 5)),
    // 20: 3->(1, 2), (5, 6)->4
    algorithm(0b00_1011, [0x04, 0x04, 0, 0x30, 0, 0], (2, 2)),
    // 21: 3->(1, 2), 6->(4, 5)
    algorithm(0b01_1011, [0x04, 0x04, 0, 0x20, 0x20, 0], (2, 2)),
    // 22: 2->1, 6->(3, 4, 5)
    algorithm(0b01_1101, [0x02, 0, 0x20, 0x20, 0x20, 0], (5, 5)),
    // 23: 3->2, 6->(4, 5)
    algorithm(0b01_1011, [0, 0x04, 0, 0x20, 0x20, 0], (5, 5)),
    // 24: 6->(3, 4, 5)
    algorithm(0b01_1111, [0, 0, 0x20, 0x20, 0x20, 0], (5, 5)),
    // 25: 6->(4, 5)
    algorithm(0b01_1111, [0, 0, 0, 0x20, 0x20, 0], (5, 5)),
    // 26: 3->2, (5, 6)->4
    algorithm(0b00_1011, [0, 0x04, 0, 0x30, 0, 0], (5, 5)),
    // 27: as 26, feedback on OP3
    algorithm(0b00_1011, [0, 0x04, 0, 0x30, 0, 0], (2, 2)),
    // 28: 2->1, 5->4->3, OP6 standing alone
    algorithm(0b10_0101, [0x02, 0, 0x08, 0x10, 0, 0], (4, 4)),
    // 29: 4->3, 6->5
    algorithm(0b01_0111, [0, 0, 0x08, 0, 0x20, 0], (5, 5)),
    // 30: 5->4->3, OP6 standing alone
    algorithm(0b10_0111, [0, 0, 0x08, 0x10, 0, 0], (4, 4)),
    // 31: 6->5
    algorithm(0b01_1111, [0, 0, 0, 0, 0x20, 0], (5, 5)),
    // 32: six independent carriers
    algorithm(0b11_1111, [0, 0, 0, 0, 0, 0], (5, 5)),
];

const fn algorithm(carriers: u8, modulators: [u8; OPERATORS], feedback: (u8, u8)) -> Algorithm {
    Algorithm {
        carriers,
        modulators,
        feedback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_descending_pass_evaluates_every_algorithm() {
        // A modulator numbered below its destination would need its output
        // before it was computed. Only the feedback path may look backwards.
        for (index, algorithm) in ALGORITHMS.iter().enumerate() {
            for (destination, sources) in algorithm.modulators.iter().enumerate() {
                for source in 0..OPERATORS {
                    if sources & (1 << source) != 0 {
                        assert!(
                            source > destination,
                            "algorithm {}: OP{} modulates OP{}",
                            index + 1,
                            source + 1,
                            destination + 1
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_operator_is_heard_or_modulates_something() {
        for (index, algorithm) in ALGORITHMS.iter().enumerate() {
            for operator in 0..OPERATORS {
                let modulates = algorithm
                    .modulators
                    .iter()
                    .any(|sources| sources & (1 << operator) != 0);
                assert!(
                    algorithm.is_carrier(operator) || modulates,
                    "algorithm {}: OP{} is connected to nothing",
                    index + 1,
                    operator + 1
                );
            }
        }
    }

    #[test]
    fn every_algorithm_has_a_carrier_and_one_feedback_path() {
        for (index, algorithm) in ALGORITHMS.iter().enumerate() {
            assert_ne!(algorithm.carriers, 0, "algorithm {} is silent", index + 1);
            assert!(algorithm.carriers < 1 << OPERATORS);
            let (source, destination) = algorithm.feedback;
            assert!(source < OPERATORS as u8 && destination < OPERATORS as u8);
            // The descending pass computes OP6 first, so a loop closes onto
            // the operator itself or onto one evaluated before it — which is
            // exactly the case where the source is not yet available and the
            // previous sample has to stand in.
            assert!(
                source <= destination,
                "algorithm {} loops forward",
                index + 1
            );
        }
    }

    #[test]
    fn the_landmark_algorithms_are_the_documented_ones() {
        // Algorithm 1 is the deep stack beside a pair; 5 is three pairs; 32 is
        // six carriers. Getting any of these wrong would mean the table was
        // transcribed against the wrong numbering.
        assert_eq!(ALGORITHMS[0].carriers, 0b00_0101);
        assert_eq!(ALGORITHMS[0].modulators[2], 0x08);
        assert_eq!(ALGORITHMS[4].carriers, 0b01_0101);
        assert_eq!(ALGORITHMS[31].carriers, 0b11_1111);
        assert_eq!(ALGORITHMS[31].modulators, [0; OPERATORS]);
        // 4 and 6 are the two algorithms whose loop spans more than one
        // operator; every other feedback is a self-loop.
        for (index, algorithm) in ALGORITHMS.iter().enumerate() {
            let (source, destination) = algorithm.feedback;
            let spans = source != destination;
            assert_eq!(
                spans,
                index == 3 || index == 5,
                "algorithm {} has the wrong kind of loop",
                index + 1
            );
        }
    }
}
