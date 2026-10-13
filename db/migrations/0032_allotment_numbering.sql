-- ═══════════════════════════════════════════════════════════════════════════
--  0032 — A document series for allotment releases
-- ═══════════════════════════════════════════════════════════════════════════
--  Allotments are released against an appropriation, and each release carries a
--  control number. Until now nothing issued that number, because there was no
--  screen to record an allotment — the budget could be seen but not entered.
--
--  Rather than let the Budget Officer type a number by hand (which is how a
--  series develops gaps and duplicates), allotment numbers are allocated the
--  same gapless way every other document is: through `next_document_number`.
--  That function refuses a doc_type the series table does not recognise, so the
--  only change needed is to admit 'ALOT'. The registry in 0019 is free-text and
--  needs no change.
-- ═══════════════════════════════════════════════════════════════════════════

ALTER TABLE document_series DROP CONSTRAINT document_series_doc_type_check;

ALTER TABLE document_series ADD CONSTRAINT document_series_doc_type_check
    CHECK (doc_type IN ('ORS','DV','JEV','OR','CHK','ADA','RCD','BRS','ALOT'));
