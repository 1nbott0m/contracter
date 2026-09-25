-- Production databases may have been created without the reference seed.
-- Keep the importer self-contained and idempotent: create the supported
-- source row before enabling it, without touching any existing source data.
INSERT INTO public.price_sources (code, display_name, enabled)
VALUES ('market_csgo', 'Market.CSGO completed sales', true)
ON CONFLICT (code) DO UPDATE
   SET display_name = EXCLUDED.display_name,
       enabled = true;
