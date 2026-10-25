ALTER TABLE recovery_codes
    ALTER COLUMN salt DROP NOT NULL,
    ALTER COLUMN code_hash DROP NOT NULL,
    ADD COLUMN code_phc text,
    ADD CONSTRAINT recovery_codes_hash_present
        CHECK (code_phc IS NOT NULL OR (salt IS NOT NULL AND code_hash IS NOT NULL));
