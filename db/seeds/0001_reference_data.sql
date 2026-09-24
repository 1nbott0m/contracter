INSERT INTO critical_action_types (code, requires_dual_approval, description) VALUES
    ('economic_settings', true, 'Change economy or risk settings'),
    ('admin_membership', true, 'Change administrator membership or rights'),
    ('early_market_unfreeze', true, 'Lift a price halt before automatic recovery'),
    ('credit_adjustment', true, 'Approve a credit adjustment above the configured threshold')
ON CONFLICT (code) DO UPDATE SET
    requires_dual_approval = EXCLUDED.requires_dual_approval,
    description = EXCLUDED.description;

INSERT INTO ledger_account_kinds (code, description) VALUES
    ('system_treasury', 'System treasury for balanced virtual-credit postings'),
    ('user_credit', 'User virtual-credit balance')
ON CONFLICT (code) DO UPDATE SET description = EXCLUDED.description;

INSERT INTO rarities (code, rank, is_covert) VALUES
    ('consumer', 0, false),
    ('industrial', 1, false),
    ('mil-spec', 2, false),
    ('restricted', 3, false),
    ('classified', 4, false),
    ('covert', 5, true)
ON CONFLICT (code) DO UPDATE SET
    rank = EXCLUDED.rank,
    is_covert = EXCLUDED.is_covert;

INSERT INTO inventory_event_kinds (code, description) VALUES
    ('test_grant', 'Initial test-only inventory grant'),
    ('market_purchase', 'Warehouse item transferred to a user'),
    ('market_buyback', 'User item returned to the warehouse'),
    ('contract_input', 'Trade-up input returned to the warehouse'),
    ('contract_output', 'Trade-up output transferred to a user'),
    ('admin_correction', 'Compensating inventory correction')
ON CONFLICT (code) DO UPDATE SET description = EXCLUDED.description;

INSERT INTO seed_event_kinds (code, description) VALUES
    ('allocated', 'Commitment allocated before quote inputs are known'),
    ('quote_created', 'Commitment attached to a completed quote'),
    ('accepted', 'Quote accepted and commitment consumed'),
    ('expired', 'Unused commitment allocation or quote expired'),
    ('rejected', 'Quote or acceptance rejected without rerolling'),
    ('revealed', 'Server seed revealed for independent replay')
ON CONFLICT (code) DO UPDATE SET description = EXCLUDED.description;

INSERT INTO quote_statuses (code, is_terminal, description) VALUES
    ('active', false, 'Quote can be accepted before expiry'),
    ('accepted', true, 'Quote was accepted exactly once'),
    ('expired', true, 'Quote expired before acceptance'),
    ('rejected', true, 'Quote was rejected without rerolling'),
    ('invalidated', true, 'Quote was invalidated by a newer valuation snapshot')
ON CONFLICT (code) DO UPDATE SET
    is_terminal = EXCLUDED.is_terminal,
    description = EXCLUDED.description;

INSERT INTO contract_statuses (code, is_terminal, description) VALUES
    ('pending', false, 'Contract is waiting for atomic finalization'),
    ('completed', true, 'Contract finalized with an auditable outcome'),
    ('failed', true, 'Contract failed without consuming its inputs')
ON CONFLICT (code) DO UPDATE SET
    is_terminal = EXCLUDED.is_terminal,
    description = EXCLUDED.description;

INSERT INTO sale_validity_reasons (code, is_valid, description) VALUES
    ('valid', true, 'Evidence is eligible for valuation'),
    ('duplicate', false, 'External sale event was already recorded'),
    ('cancelled', false, 'Source reports a cancelled sale'),
    ('future_dated', false, 'Source timestamp is later than receipt time'),
    ('wrong_currency', false, 'Sale currency is not supported by the policy'),
    ('wrong_variant', false, 'Sale is not the normal non-StatTrak non-Souvenir variant'),
    ('anomaly', false, 'Evidence failed a configured anomaly check')
ON CONFLICT (code) DO UPDATE SET
    is_valid = EXCLUDED.is_valid,
    description = EXCLUDED.description;

INSERT INTO price_halt_reasons (code, description) VALUES
    ('movement_24h', 'Verified price moved by more than 20 percent in 24 hours'),
    ('insufficient_sales', 'Fewer than 20 valid sales exist in the allowed windows'),
    ('minimum_notional', 'Sale evidence is below the calibrated minimum notional'),
    ('dispersion', 'Sale evidence dispersion exceeds the calibrated limit'),
    ('manual', 'Two administrators applied a manual halt')
ON CONFLICT (code) DO UPDATE SET description = EXCLUDED.description;

INSERT INTO currencies (code, minor_unit_scale)
VALUES ('USD', 2)
ON CONFLICT (code) DO UPDATE SET minor_unit_scale = EXCLUDED.minor_unit_scale;

INSERT INTO grant_kinds (code, description) VALUES
    ('test_access_1000_usd', 'One-time test-environment grant of 1000 CC in virtual credits')
ON CONFLICT (code) DO UPDATE SET description = EXCLUDED.description;

INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) VALUES
    ('factory_new', 0.00000000, 0.07000000, false),
    ('minimal_wear', 0.07000000, 0.15000000, false),
    ('field_tested', 0.15000000, 0.38000000, false),
    ('well_worn', 0.38000000, 0.45000000, false),
    ('battle_scarred', 0.45000000, 1.00000000, true)
ON CONFLICT (code) DO UPDATE SET
    lower_bound = EXCLUDED.lower_bound,
    upper_bound = EXCLUDED.upper_bound,
    includes_upper_bound = EXCLUDED.includes_upper_bound;

INSERT INTO collections (public_id, slug, display_name, enabled) VALUES
    ('81000000-0000-0000-0000-000000000001', 'control', 'Control Collection', true),
    ('81000000-0000-0000-0000-000000000002', 'ancient', 'Ancient Collection', true),
    ('81000000-0000-0000-0000-000000000003', 'havoc', 'Havoc Collection', true),
    ('81000000-0000-0000-0000-000000000004', '2021-dust-2', '2021 Dust 2 Collection', true),
    ('81000000-0000-0000-0000-000000000005', '2021-mirage', '2021 Mirage Collection', true),
    ('81000000-0000-0000-0000-000000000006', '2021-vertigo', '2021 Vertigo Collection', true),
    ('81000000-0000-0000-0000-000000000007', 'canals', 'Canals Collection', true),
    ('81000000-0000-0000-0000-000000000008', 'norse', 'Norse Collection', true),
    ('81000000-0000-0000-0000-000000000009', '2021-train', '2021 Train Collection', true),
    ('81000000-0000-0000-0000-000000000010', '2018-nuke', '2018 Nuke Collection', true)
ON CONFLICT (slug) DO UPDATE SET
    display_name = EXCLUDED.display_name,
    enabled = EXCLUDED.enabled;

INSERT INTO stock_policy_versions (public_id, version, activated_at)
VALUES ('82000000-0000-0000-0000-000000000001', 1, clock_timestamp())
ON CONFLICT (version) DO NOTHING;

INSERT INTO stock_policy_bands (
    stock_policy_version_id,
    rarity_code,
    minimum_units,
    target_units,
    maximum_units
)
SELECT policy.id, target.rarity_code, 0, target.target_units, target.target_units
FROM stock_policy_versions AS policy
CROSS JOIN (VALUES
    ('consumer'::text, 167),
    ('industrial'::text, 125),
    ('mil-spec'::text, 83),
    ('restricted'::text, 42),
    ('classified'::text, 17),
    ('covert'::text, 7)
) AS target(rarity_code, target_units)
WHERE policy.version = 1
ON CONFLICT (stock_policy_version_id, rarity_code) DO UPDATE SET
    minimum_units = EXCLUDED.minimum_units,
    target_units = EXCLUDED.target_units,
    maximum_units = EXCLUDED.maximum_units;

-- The four approved risk ratios are seeded, but this version intentionally
-- remains inactive. Minimum notional and dispersion still require calibration
-- against real sale evidence before production activation.
INSERT INTO risk_policy_versions (
    public_id,
    version,
    minimum_coverage_ratio,
    maximum_quote_reserve_ratio,
    maximum_item_liability_ratio,
    maximum_collection_liability_ratio,
    minimum_notional_microcredits,
    maximum_dispersion_ratio,
    activated_at
) VALUES (
    '83000000-0000-0000-0000-000000000001',
    1,
    1.25000000,
    0.01000000,
    0.10000000,
    0.25000000,
    0,
    1.00000000,
    NULL
)
ON CONFLICT (version) DO NOTHING;

INSERT INTO price_sources (code, display_name, enabled)
VALUES ('market_csgo', 'Market.CSGO completed sales', false)
ON CONFLICT (code) DO UPDATE SET display_name = EXCLUDED.display_name;
