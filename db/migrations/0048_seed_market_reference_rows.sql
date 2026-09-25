-- Production may be running without the optional reference seed.  The
-- importer only writes evidence classified as valid, so guarantee that
-- referenced reason exists before accepting source data.
INSERT INTO public.sale_validity_reasons (code, is_valid, description)
VALUES ('valid', true, 'Evidence is eligible for valuation')
ON CONFLICT (code) DO UPDATE
   SET is_valid = true,
       description = EXCLUDED.description;
