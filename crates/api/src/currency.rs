/// Public internal wallet and economy currency code.
pub const INTERNAL_CURRENCY_CODE: &str = "CC";
/// One CC is represented by one million integer micro-CC units in the ledger.
pub const MICRO_UNITS_PER_CC: i64 = 1_000_000;

#[cfg(test)]
mod tests {
    #[test]
    fn internal_money_is_contracter_coins() {
        assert_eq!(super::INTERNAL_CURRENCY_CODE, "CC");
        assert_eq!(super::MICRO_UNITS_PER_CC, 1_000_000);
    }
}
