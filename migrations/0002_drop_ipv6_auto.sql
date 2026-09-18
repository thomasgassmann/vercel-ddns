ALTER TABLE sync_runs DROP COLUMN ipv6;
UPDATE records SET value = '::' WHERE record_type = 'AAAA' AND value IS NULL;
ALTER TABLE records DROP CONSTRAINT records_value_check;
ALTER TABLE records ADD CONSTRAINT records_value_check CHECK (value IS NOT NULL OR record_type = 'A');
