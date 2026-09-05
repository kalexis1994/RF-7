//! Bessel functions of the first kind, integer order.
//!
//! `J_n(x)` is the amplitude of the *n*-th sideband of a sinusoid whose phase
//! is modulated sinusoidally with index `x`. The power series converges for
//! every finite `x`; for the indexes an FM operator can reach — below about
//! thirty — double precision loses nothing to the alternating terms.

/// `J_n(x)` for any integer `n`, using `J_{-n} = (-1)^n J_n`.
pub fn bessel_j(order: i32, x: f64) -> f64 {
    let n = order.unsigned_abs();
    let value = positive_order(n, x);
    if order < 0 && n % 2 == 1 {
        -value
    } else {
        value
    }
}

fn positive_order(n: u32, x: f64) -> f64 {
    let half = x / 2.0;
    // First term: (x/2)^n / n!, built as a product so it cannot overflow the
    // way a factorial alone would.
    let mut term = 1.0;
    for k in 1..=n {
        term *= half / f64::from(k);
    }
    let mut sum = term;
    let ratio = -half * half;
    for k in 1..200u32 {
        term *= ratio / (f64::from(k) * f64::from(n + k));
        sum += term;
        if term.abs() < 1e-17 * sum.abs().max(1e-300) {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_from_the_tables() {
        assert!((bessel_j(0, 0.0) - 1.0).abs() < 1e-15);
        assert!((bessel_j(1, 0.0)).abs() < 1e-15);
        assert!((bessel_j(0, 1.0) - 0.765_197_686_6).abs() < 1e-9);
        assert!((bessel_j(1, 1.0) - 0.440_050_585_7).abs() < 1e-9);
        assert!((bessel_j(2, 2.5) - 0.446_059_058_4).abs() < 1e-9);
        assert!(
            (bessel_j(0, 2.404_825_557_7)).abs() < 1e-9,
            "the first zero of J0"
        );
        assert!((bessel_j(5, 10.0) + 0.234_061_528_2).abs() < 1e-9);
    }

    #[test]
    fn negative_orders_alternate_in_sign() {
        for x in [0.3, 1.7, core::f64::consts::TAU, 12.0] {
            assert_eq!(bessel_j(-1, x), -bessel_j(1, x));
            assert_eq!(bessel_j(-2, x), bessel_j(2, x));
            assert_eq!(bessel_j(-3, x), -bessel_j(3, x));
        }
    }

    #[test]
    fn the_sidebands_of_any_index_carry_unit_power() {
        // Parseval for FM: the squares of every J_n sum to one, which is what
        // lets the estimator normalise a measured spectrum against the model.
        // The alternating series loses about a digit per five units of x to
        // cancellation; an operator never reaches an index of twenty-five,
        // so the looser bound there is the series' limit, not the model's.
        for (x, tolerance) in [
            (0.5, 1e-12),
            (2.0, 1e-12),
            (core::f64::consts::TAU, 1e-10),
            (15.0, 1e-8),
            (25.0, 1e-6),
        ] {
            let power: f64 = (-60..=60).map(|n| bessel_j(n, x).powi(2)).sum();
            assert!(
                (power - 1.0).abs() < tolerance,
                "index {x} summed to {power}"
            );
        }
    }
}
