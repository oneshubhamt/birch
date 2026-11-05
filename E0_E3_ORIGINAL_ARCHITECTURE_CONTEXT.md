I audited the current **1a.34** source. Based on the code as it exists now and everything we discussed, I would **not touch SQLite, cloud sync, PWA, or whole-database encryption** at this stage.
The architecture I would pursue is:

```
Rustnote
│
├── Normal content
│   └── existing SQLite + attachments + FTS5
│
├── Rustnote Vault
│   ├── encrypted note
│   └── encrypted notebook
│
└── Native app lock
    └── Touch ID / Windows Hello / Android biometrics
```

## What the audit tells me

The current architecture is actually quite suitable for this.
First, **SQLite and FTS5 should remain exactly where they are**. Search is already separated reasonably well into the search parser, storage-side FTS translation, and application services. I see no reason to disturb the database engine.
Second, note content currently exists in several places:

```
title
Markdown body
ProseMirror/Wordgard JSON
plain-text projection
FTS5
revision snapshots
emergency autosave journal
backups
```

So proper encryption cannot mean merely:

> encrypt `notes.body`

It must mean:

> **nothing belonging to a locked note is accidentally left somewhere else in plaintext.**

Third, attachments need special care. Rustnote currently stores attachments by their SHA-256 content identity and also produces temporary/cached copies for previews, compatibility conversion and OS handoff. We therefore need a proper encrypted-attachment path rather than simply encrypting the existing shared file in place.
Fourth, your PDF observation is correct. The current PDF viewer already has its own text/search machinery, but **PDF text is not part of Rustnote's global FTS search**.
That is something I would fix as part of the broader search architecture.

---

# What I would implement first

I would start with **individual note encryption**, but make that one feature completely correct before adding encrypted notebooks.

### Pass E1 — Rustnote Vault foundation

Create one small Rust service:

```
VaultService
```

It owns only:

- encryption/decryption;
- key generation;
- password-to-key protection;
- encrypted file format;
- secure key lifetime;
- protected attachment streaming.

No encryption logic in the UI.
No encryption logic scattered throughout storage.
No JavaScript crypto.
For the cipher, I would **not hard-code the decision before benchmarking**.
I would benchmark:

```
AEGIS-256
XChaCha20-Poly1305
```

on the relevant architectures and choose one canonical Rustnote v1 cipher.
There is now a maintained Rust `aegis` crate with AEGIS-256 support, including incremental/random-access encrypted file facilities and WebAssembly support. ([Docs.rs](https://docs.rs/aegis/latest/aegis/?utm_source=chatgpt.com)) XChaCha20-Poly1305 also has a mature current RustCrypto implementation with a 192-bit nonce. ([Docs.rs](https://docs.rs/chacha20poly1305/latest/chacha20poly1305/?utm_source=chatgpt.com)) Libsodium currently recommends **AEGIS-256 when hardware AES acceleration exists and XChaCha20-Poly1305 otherwise**. ([libsodium Documentation](https://doc.libsodium.org/secret-key_cryptography/aead?utm_source=chatgpt.com))
My present inclination is still:

> **AEGIS-256 if our real benchmarks and portability tests are clean; XChaCha20-Poly1305 otherwise.**

But I would decide from evidence, not preference.
The encrypted Rustnote format itself would be versioned:

```
Rustnote Encrypted Object
version: 1
algorithm: AEGIS-256
...
```

so we can migrate algorithms decades later.

---

# Pass E2 — one genuinely encrypted note

This would be the first user-visible feature.
When you choose:

> **Encrypt Note**

Rustnote generates a random note key.
Then it encrypts everything sensitive belonging to that note.
Conceptually:

```
🔒 Note
│
├── title
├── document
├── tags
├── revisions
├── sources/citations
├── PDF annotations
├── attachment metadata
└── attachments themselves
```

The ordinary SQLite note row would retain only enough information for Rustnote to know:

> “A locked note exists here.”

For example:

```
note ID
notebook ID
locked = yes
basic structural state
```

The UI could simply display:

> 🔒 Locked note

rather than storing the real title.

---

## Attachments

Normal notes continue using today's attachment system unchanged.
Encrypted notes get protected blobs:

```
ordinary attachment
→ existing SHA-256 blob system

protected attachment
→ encrypted protected blob
```

That separation is important because the current Rustnote attachment architecture deduplicates/content-addresses files.
We should **not encrypt an ordinary shared blob in place**.
Large PDFs/videos/audio remain streamed in chunks. Encryption must never require loading a 2 GB file into RAM.

---

# Pass E3 — close every plaintext leak

Before declaring note encryption complete, I would explicitly audit and fix these surfaces:

```
FTS5
revision history
emergency journal
automatic backups
portable backup/export
image rendition cache
media compatibility cache
attachment handoff
PDF annotations
preview files
temporary attachment staging
```

For example, the emergency journal currently contains a note's title and editor state. A protected note's emergency recovery copy therefore must also be encrypted.
Likewise, decrypted protected attachments must never quietly remain forever inside a compatibility cache.
This pass is particularly important. **The cipher is only one part of security; plaintext leakage elsewhere can defeat excellent encryption.**

---

# Then I would improve search

This should probably be the next substantial pass because encryption and search are closely related.
I would introduce one application-level:

```
SearchService
```

while keeping FTS5 as the actual engine.
It would combine two indexes.

### Persistent search

For ordinary content:

```
SQLite FTS5
```

Exactly as today, but refined.

### Private-session search

For unlocked encrypted content:

```
SQLite :memory: FTS5
```

Therefore:

```
Locked note
→ absent from search

Unlock note
→ decrypted text enters RAM search index

Lock note
→ RAM entries destroyed
```

No decrypted encrypted-note search index touches disk.

---

# Global PDF search

I would implement this at the same time.
Today:

```
Note text → global search ✓

PDF text → PDF viewer search only
```

I would change it to:

```
                    Rustnote Search
                         │
          ┌──────────────┼──────────────┐
          │              │              │
       Notes           Tags           PDFs
                                        │
                                     page text
```

PDF.js already has the underlying PDF text layer machinery.
On import/background indexing:

```
PDF
 ↓
PDF.js extracts page text
 ↓
Rustnote stores page-search text
 ↓
FTS5
```

Then:

> **Search: "Kesavananda Bharati"**

could produce:

```
Notes
Constitutional Law

PDFs
Indian Polity.pdf — p. 241
Constitution Cases.pdf — p. 87
```

Clicking a PDF result opens that PDF directly at the relevant page.
For an encrypted PDF:

```
locked
→ PDF text not searchable

unlocked
→ page text enters RAM FTS
→ globally searchable
```

Scanned image-only PDFs can later receive optional OCR. I would keep OCR as a separate later feature.

---

# Then encrypted notebooks

Once one encrypted note works perfectly, notebooks become much safer to implement.
Internally:

```
Encrypted Notebook
      │
 Notebook Key
      │
 ┌────┼────┐
 │    │    │
N1   N2   N3
```

Each note still gets its own random data key.
The notebook key controls access to them.
This gives us an important advantage: **we do not repeatedly encrypt huge attachments when reorganising notes**.
For example, moving a note containing a 5 GB video between encrypted notebooks should primarily require changing protected key metadata, not decrypting and re-encrypting 5 GB.
And I would avoid pointless double encryption:

> A note inside an encrypted notebook is protected by the notebook scope. It doesn't need another redundant layer unless the user explicitly wants an independently locked note.

---

# Only after that: native app lock

This is much simpler.
It is **not another Rustnote encryption system**.
It is:

```
Open Rustnote
      ↓
Touch ID / Windows Hello / Android biometric
      ↓
allow application access
```

The OS handles it.
Rustnote's own cryptography continues to deal only with:

```
🔒 notes
🔒 notebooks
```

This matches your revised idea much better.

---

# And only much later: cloud sync

By postponing sync, we actually make it easier.
When Google Drive support eventually arrives:

```
ordinary content
→ encrypted before cloud upload

already protected content
→ remains protected

Google Drive
→ sees ciphertext
```

The Vault format we build now becomes part of the eventual sync security foundation.
No need to redesign encryption around Google later.

---

# So my actual implementation sequence would be

|   |
| - |

Pass
Objective

|   |
| - |

**E0**

|   |
| - |

Crypto benchmark + exact privacy/threat-model contract

|   |
| - |

**E1**

|   |
| - |

One canonical `VaultService`, encrypted-object format and key management

|   |
| - |

**E2**

|   |
| - |

Completely encrypt **one note**, including its attachments

|   |
| - |

**E3**

|   |
| - |

Plaintext-leak audit: revisions, autosave, backups, caches, exports, previews

|   |
| - |

**S1**

|   |
| - |

Refine global search architecture

|   |
| - |

**S2**

|   |
| - |

Add global PDF/page search

|   |
| - |

**E4**

|   |
| - |

RAM-only search for unlocked encrypted content

|   |
| - |

**E5**

|   |
| - |

Encrypted notebooks and notebook→note key hierarchy

|   |
| - |

**E6**

|   |
| - |

Native OS app lock

|   |
| - |

Later

|   |
| - |

Google Drive/cloud sync

|   |
| - |

Later

|   |
| - |

PWA

|   |
| - |

Much later if useful

|   |
| - |

PQ device-to-device key exchange / signed revision chains

### The key architectural decision

I would **not rewrite Rustnote around encryption**.
The goal would be:

```
Existing Rustnote
      +
one narrow Vault capability
      +
one stronger Search capability
```

SQLite remains SQLite.
FTS5 remains FTS5.
The editor remains unchanged.
PDF.js remains the PDF engine.
The existing attachment system remains the ordinary attachment system.
We add protected paths alongside them instead of destabilising mature code.
That is the implementation direction I would recommend from this exact 1a.34 baseline.