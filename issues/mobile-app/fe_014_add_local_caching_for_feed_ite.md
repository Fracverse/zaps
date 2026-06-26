# MOBILE-APP [FE-014]: Add local caching for feed items using AsyncStorage

## Description
Cache feed locally to load content instantly on startup.

## Files to Edit/Create
- `mobileapp/app/(personal)/home.tsx`

## Acceptance Criteria
- Load cached items immediately, then fetch updates from backend.

## Guidance / Hints
Read feed items from AsyncStorage inside useEffect on mount.
