---
id: CACHE-SERVES-STALE
title: The cache serves stale values after expiration
description: An expired value is returned without recomputation.
status: draft
---
# The cache serves stale values after expiration

## Problem

Expiration is never checked when the cache is read, so an expired value is served.
