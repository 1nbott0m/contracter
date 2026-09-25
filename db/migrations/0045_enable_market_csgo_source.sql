-- The Market.CSGO importer is now an explicitly supported production source.
-- Writes still require the SECURITY DEFINER ingest function and admin runtime role.
UPDATE price_sources
   SET enabled = true
 WHERE code = 'market_csgo';
