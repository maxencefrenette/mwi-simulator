#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceBinDirection {
    Down,
    Up,
}

pub fn bin_market_price(price: f64, direction: PriceBinDirection, vendor_price: f64) -> f64 {
    let price = finite_nonnegative_integer(price);
    if price <= 1 {
        return 2.0;
    }

    let step = market_price_step(price);
    let remainder = price % step;
    let lower = price - remainder;
    let round_up =
        direction == PriceBinDirection::Up || lower < finite_nonnegative_integer(vendor_price);

    if round_up && remainder > 0 {
        (lower + step) as f64
    } else {
        lower as f64
    }
}

pub fn market_price_step(price: u64) -> u64 {
    if price <= 1 {
        return 1;
    }

    let digits = price.ilog10() + 1;
    let magnitude = 10_u64.pow(digits - 1);
    let leading_digit = price / magnitude;

    // The client widens bins as prices grow, with a different multiplier per leading digit.
    match leading_digit {
        1 | 2 if digits >= 3 => 5 * 10_u64.pow(digits - 3),
        3 | 4 if digits >= 2 => 10_u64.pow(digits - 2),
        5..=9 if digits >= 2 => 2 * 10_u64.pow(digits - 2),
        _ => 1,
    }
}

fn finite_nonnegative_integer(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.floor() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_client_price_bins() {
        assert_eq!(bin_market_price(1.0, PriceBinDirection::Up, 0.0), 2.0);
        assert_eq!(bin_market_price(49.0, PriceBinDirection::Up, 0.0), 49.0);
        assert_eq!(bin_market_price(51.0, PriceBinDirection::Down, 0.0), 50.0);
        assert_eq!(bin_market_price(51.0, PriceBinDirection::Up, 0.0), 52.0);
        assert_eq!(bin_market_price(246.0, PriceBinDirection::Down, 0.0), 245.0);
        assert_eq!(bin_market_price(246.0, PriceBinDirection::Up, 0.0), 250.0);
        assert_eq!(bin_market_price(541.0, PriceBinDirection::Up, 0.0), 560.0);
        assert_eq!(
            bin_market_price(1_251.0, PriceBinDirection::Up, 0.0),
            1_300.0
        );
    }

    #[test]
    fn rounds_up_when_the_lower_bin_is_below_vendor_price() {
        assert_eq!(
            bin_market_price(246.0, PriceBinDirection::Down, 248.0),
            250.0
        );
    }
}
