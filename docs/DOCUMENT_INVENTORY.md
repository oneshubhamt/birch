# Birch documentation inventory — baseline snapshot

> **Metadata inventory only.** This document catalogs every Markdown file found in the `main` Git tree at baseline commit `514b797fb0a6a128050cf75ac2428fc12095106a` (2026-11-14 inventory). A provisional family label does **not** establish semantic authority, correctness, current applicability, duplication, or supersession. No historical file has been rewritten to produce this inventory.

**Baseline: 239 Markdown files**, 226 at repository root. Baseline Markdown total: 895,754 bytes (Git tree size).

**Preservation:** Keep all baseline document paths unchanged until reviewed against their sources, their successors, and validators referring to exact filenames. Use [Consolidation plan](CONSOLIDATION_PLAN.md) before any migration or deletion.

## Families

| Provisional family | Files | Bytes |
| --- | ---: | ---: |
| Living guides and notices | 8 | 19,556 |
| Baseline setup evidence and instructions | 2 | 4,538 |
| Candidate governing security and backup contracts | 6 | 114,530 |
| Original design context | 1 | 10,709 |
| Vault design, implementation and closure history | 37 | 217,680 |
| Security implementation, remediation and closure history | 14 | 83,879 |
| Backup implementation history | 1 | 10,212 |
| QA reports and historical evidence | 104 | 346,815 |
| Native compile implementation and plan history | 58 | 76,558 |
| Version and promotion history | 1 | 1,255 |
| Component guides, templates and bundled content | 7 | 10,022 |

## Full inventory

Every row below records the **existing original path**, Git-reported file size, and original Git **blob SHA**. These are version identifiers for change detection, not independently computed SHA-256 security checks.

### Living guides and notices (8)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`CONTRIBUTING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/CONTRIBUTING.md) | 2101 | `734091650837974d42559e0d0b4a0f85edb991e3` |
| [`docs/ARCHITECTURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/ARCHITECTURE.md) | 2267 | `5c4ab4c86175a6ef2cd69c111a560218d9d8a0bc` |
| [`docs/DEVELOPMENT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/DEVELOPMENT.md) | 2468 | `963ea2ec07b2d0415af484ba9fa9ae9bc3da32a5` |
| [`docs/PROJECT_STATUS.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/PROJECT_STATUS.md) | 1197 | `4b0d9ef2801360bd54c93230bd77799ffee7a035` |
| [`docs/QA.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/QA.md) | 2047 | `076d76ffa1b3b609f87b28a5060a3c20c554803c` |
| [`README.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/README.md) | 3943 | `c88ce7454bf9bf46b97daace8a159f6b7b97846c` |
| [`SECURITY.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY.md) | 2012 | `3652c8f83f010d01e3ff8c5d7419833f315fa199` |
| [`THIRD_PARTY_NOTICES.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/THIRD_PARTY_NOTICES.md) | 3521 | `17ca9021d1fbda7a5757055511c48da2f42915f5` |

### Baseline setup evidence and instructions (2)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`docs/BOOTSTRAP_QA.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/BOOTSTRAP_QA.md) | 1808 | `69215ed1470f99db1ec3fdd06fcd36f367b569bb` |
| [`docs/REPOSITORY_SETUP.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/REPOSITORY_SETUP.md) | 2730 | `88353ca71fbb0d542d674443c62aacf9b8bff9f8` |

### Candidate governing security and backup contracts (6)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md) | 27430 | `daa051416b223c3de07f7223b961d150e7fb0be7` |
| [`BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md) | 28249 | `bdc8606d8b73acc53e91fa2a69e15b0c33c0ad47` |
| [`SECURITY_ENCRYPTION_E0_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_ENCRYPTION_E0_CONTRACT.md) | 10806 | `1daa3d82d081e4c6b35d72b4be85e55acd0d621a` |
| [`SECURITY_ENCRYPTION_E1_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_ENCRYPTION_E1_CONTRACT.md) | 6782 | `739c2b248f00614054dad9e305ca276585cd496e` |
| [`SECURITY_ENCRYPTION_E2_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_ENCRYPTION_E2_CONTRACT.md) | 8316 | `36818fcb02e43c9c46482f7b6e25532739d8b473` |
| [`SECURITY_ENCRYPTION_E3_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_ENCRYPTION_E3_CONTRACT.md) | 32947 | `547fd93a2772405b621340f79c36417ab9de35bc` |

### Original design context (1)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`E0_E3_ORIGINAL_ARCHITECTURE_CONTEXT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/E0_E3_ORIGINAL_ARCHITECTURE_CONTEXT.md) | 10709 | `1ce0df7a5e5ba4acfdae0438cec5245c1bf08a13` |

### Vault design, implementation and closure history (37)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`VAULT_2B1_2B4_FINAL_CLOSURE_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_2B4_FINAL_CLOSURE_AUDIT_PLAN.md) | 1469 | `a226d9c0e65dd24ad743a9e0c5cbc72f4804aca1` |
| [`VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md) | 8245 | `b386db8c8eba28e9e4e3dc6b101b14fa22bdc904` |
| [`VAULT_2B1_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_AUDIT_PLAN.md) | 4449 | `d1bf0c15422e8765f83901588e31aa0bbd473e90` |
| [`VAULT_2B1_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_IMPLEMENTATION.md) | 4936 | `5da013391fea280234192611147a1ee6153be8ca` |
| [`VAULT_2B10_CROSS_CUTTING_INVARIANT_CLOSURE_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B10_CROSS_CUTTING_INVARIANT_CLOSURE_PLAN.md) | 1935 | `ef551f59c78e415ff0f7cec4cb8e20fca1bf4901` |
| [`VAULT_2B10_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B10_IMPLEMENTATION.md) | 4906 | `e65698f43ed8cf09fc40a0116b42946645908aa2` |
| [`VAULT_2B11_ATOMIC_PRIVACY_AUTHORITY_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B11_ATOMIC_PRIVACY_AUTHORITY_CLOSURE.md) | 5728 | `eea8dc5652d744047dc49550f27a636a5d109519` |
| [`VAULT_2B12_AEGIS_PRIVACY_DISCLOSURE_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B12_AEGIS_PRIVACY_DISCLOSURE_CLOSURE.md) | 5341 | `18c5c2b133294228c11919eaf9c6819b1f05a697` |
| [`VAULT_2B2_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B2_IMPLEMENTATION.md) | 8755 | `e570843808520b55add07b99db8930a8d88e3387` |
| [`VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md) | 4756 | `a1765f16f31c354ed5c516e8d84d87bd934c836a` |
| [`VAULT_2B3_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B3_AUDIT_PLAN.md) | 3228 | `61ea539104b92d9bede702638068d86ae9cb4e64` |
| [`VAULT_2B3_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B3_IMPLEMENTATION.md) | 13229 | `7bdd6efde0c8d00c3c654dc747036d6a7194ab95` |
| [`VAULT_2B4_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B4_AUDIT_PLAN.md) | 4069 | `cb5a2e40b7fe014012f9a0051823907516ab4229` |
| [`VAULT_2B4_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B4_IMPLEMENTATION.md) | 6322 | `f3b77d591ef615e82d1e0627433535589c4f921f` |
| [`VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md) | 2708 | `dae37aa98d6fa68542453397ea4fe2216b6e4b44` |
| [`VAULT_2B42_10_INDEPENDENT_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_10_INDEPENDENT_CLOSURE.md) | 11883 | `dda9c249eecd3db1b8ed23e7b59bb1e125283d58` |
| [`VAULT_2B42_10_WIP_HANDOFF.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_10_WIP_HANDOFF.md) | 11413 | `33a8cbbe1c6109c086f3c8a64dc30b9d1ccb9922` |
| [`VAULT_2B42_11_FINAL_QA_PACKAGE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_11_FINAL_QA_PACKAGE.md) | 4951 | `8591aadf34aa436396692134f020f98c6ed1134b` |
| [`VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md) | 3543 | `485ee41a282044d88d3699b68eac9d3ba447668f` |
| [`VAULT_2B42_3_NOTE_LOCAL_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_3_NOTE_LOCAL_UI.md) | 2061 | `65a7b87b02def0fe2db5e8be197e62096e21c483` |
| [`VAULT_2B42_4_MULTI_NOTE_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_4_MULTI_NOTE_UI.md) | 3288 | `9e1182eb76c280399fe72f4ee16c1a1735c7e7c8` |
| [`VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md) | 7433 | `4ee99d7f7228ba3d5d5c022f214956d6535a1f69` |
| [`VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md) | 11949 | `5cdaf8c88f9dbceabe000be9bd41375419b071a0` |
| [`VAULT_2B42_7_PROFILING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_7_PROFILING.md) | 3261 | `2df9dca43346579b235e3330fdec9f10a1b4ed7f` |
| [`VAULT_2B42_8_NO_VACUUM_FORENSIC.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_8_NO_VACUUM_FORENSIC.md) | 3863 | `f24e797d9857f297e6ec8597a1445a2a4e83411d` |
| [`VAULT_2B42_9_CLEANUP.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_9_CLEANUP.md) | 3349 | `c39b1a53b099511af568b86945619c48df148cb9` |
| [`VAULT_2B5_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B5_AUDIT_PLAN.md) | 3627 | `7263265491a0854f28d84f74c9607e79174dcff7` |
| [`VAULT_2B5_FINAL_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B5_FINAL_AUDIT_PLAN.md) | 4693 | `f715752e51cd0465a0621b290e01ed0242ce59bb` |
| [`VAULT_2B5_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B5_IMPLEMENTATION.md) | 12135 | `930d33d07bc9ed61425da31e5777c184266682c1` |
| [`VAULT_2B6_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B6_AUDIT_PLAN.md) | 3492 | `0482adbb911b282487361150a120f34e9e047696` |
| [`VAULT_2B6_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B6_IMPLEMENTATION.md) | 13451 | `c9a577c6bbf94830839bc99eb8857e85a13e13f0` |
| [`VAULT_2B7_CLOSURE_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B7_CLOSURE_AUDIT_PLAN.md) | 6058 | `e2859b0c8218231c51f42bbeddc3bdda785f5f99` |
| [`VAULT_2B7_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B7_IMPLEMENTATION.md) | 7287 | `7946e95446d53ed90d1cf71b56d84ff47f997a4c` |
| [`VAULT_2B8_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B8_IMPLEMENTATION.md) | 5131 | `bdc3607af8d3c00974ef4736e2bac768c87b8a5e` |
| [`VAULT_2B8_PLAINTEXT_AUTHORITY_CLOSURE_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B8_PLAINTEXT_AUTHORITY_CLOSURE_PLAN.md) | 4137 | `acb899c23c7ed0aaf2a03c619331a8708e9d58c9` |
| [`VAULT_2B9_BACKUP_OWNERSHIP_CLOSURE_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B9_BACKUP_OWNERSHIP_CLOSURE_PLAN.md) | 5140 | `7b8ba6ffabdbde557c5255e3dd960ebb8ab88c68` |
| [`VAULT_2B9_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B9_IMPLEMENTATION.md) | 5459 | `d56e383ee07bf9279e22aca230850731996941f4` |

### Security implementation, remediation and closure history (14)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`SECURITY_E0_E3_CLOSURE_2A10.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A10.md) | 3940 | `4904c7a7ac6c12d9b811f199871c2c1c79c01f4e` |
| [`SECURITY_E0_E3_CLOSURE_2A11.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A11.md) | 5436 | `26cacf4e06a41f0085dd32f4ae57990ad6d8436d` |
| [`SECURITY_E0_E3_CLOSURE_2A12.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A12.md) | 3455 | `058207a3ae3ad5bd143017b0d71ca578971b29a6` |
| [`SECURITY_E0_E3_CLOSURE_2A13.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A13.md) | 4415 | `e14ca5f361e6494f2c44336e20399d34b2d137be` |
| [`SECURITY_E0_E3_CLOSURE_2A14.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A14.md) | 7112 | `054b0df769dcc5a14520a3275d0d6e46b5cb5822` |
| [`SECURITY_E0_E3_CLOSURE_2A19.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A19.md) | 15231 | `20dc5a408e51ee6acb23d3cf38f7e3832b81aaf6` |
| [`SECURITY_E0_E3_CLOSURE_2A6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A6.md) | 6997 | `ec9c2d72b4868e7446c3bedddd3e333201bf7aab` |
| [`SECURITY_E0_E3_CLOSURE_2A7.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A7.md) | 7949 | `e3325f027971726383f3ff32a466f5cc721e3544` |
| [`SECURITY_E0_E3_CLOSURE_2A8.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A8.md) | 5137 | `4c444cfee2c8a87f42419e0851b68e9a87a55c0a` |
| [`SECURITY_E0_E3_CLOSURE_2A9.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A9.md) | 3174 | `554a13ca53ca0767398344e212421d8d0f877052` |
| [`SECURITY_E0_E3_REMEDIATION_2A15.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A15.md) | 2992 | `48b9836c86ff21acd611cd0c068fa4ce4f9afd44` |
| [`SECURITY_E0_E3_REMEDIATION_2A16.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A16.md) | 8381 | `22791d25e78e0f6f3d63c3f40786db127f83bc96` |
| [`SECURITY_E0_E3_REMEDIATION_2A17.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A17.md) | 4573 | `48886c7638347edce1a7886b148139c7fd46a6f0` |
| [`SECURITY_E0_E3_REMEDIATION_2A18.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A18.md) | 5087 | `4dfd9ec5540e7cc4e454fc05a49a6b844d1d3ce2` |

### Backup implementation history (1)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md) | 10212 | `2d2acf1dfcdd5acdcb9e233ae6dd6d075f30631c` |

### QA reports and historical evidence (104)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`QA_ARCHITECTURE_2A5_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_2A5_CLOSURE.md) | 1493 | `93bca9d5380caf1d6e240bdf4cf26854724728e7` |
| [`QA_ARCHITECTURE_A0.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A0.md) | 1907 | `d7c8ecdb880481237dfc5c2811b628b97170947a` |
| [`QA_ARCHITECTURE_A1_A4_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A1_A4_CLOSURE.md) | 2145 | `68fd21601d9e652c26935604e8c27cbc41dc420e` |
| [`QA_ARCHITECTURE_A1.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A1.md) | 2641 | `0c63ad23afe75eccd6d7e44f58035334881dc561` |
| [`QA_ARCHITECTURE_A2.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A2.md) | 2282 | `a8596b83914b12663b4222f33747824e1a3c0d42` |
| [`QA_ARCHITECTURE_A3.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A3.md) | 3580 | `74d6ec9db0c29b40551d95b0fe731613a9b10591` |
| [`QA_ARCHITECTURE_A4.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A4.md) | 3253 | `79298f8cf2e82ee2be7de88772ec38839c69b07e` |
| [`QA_ARCHITECTURE_A5.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A5.md) | 4117 | `c2f9e69f905abc6b85f0dcc7d771a85c068f5cbb` |
| [`QA_ARCHITECTURE_A6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A6.md) | 2057 | `12ae05d57f5c7c8ab87f348eaea18e59b1056f81` |
| [`QA_ARCHITECTURE_A7.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_A7.md) | 3229 | `d637d33773909a39f26f9ee1c5dc3020e4270420` |
| [`QA_ARCHITECTURE_B1.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B1.md) | 6269 | `1101171dd75fab12cf35eb0f4672fe0b16e52135` |
| [`QA_ARCHITECTURE_B2.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B2.md) | 4848 | `cc91c78af3cd463adcd6f08a18abb7aaf14841bd` |
| [`QA_ARCHITECTURE_B3.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B3.md) | 4483 | `8fbd308063339b0bea7765d08c8533949591de52` |
| [`QA_ARCHITECTURE_B4.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B4.md) | 4139 | `8555420e925369f45217879b74fb4569ce3acd8d` |
| [`QA_ARCHITECTURE_B5.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B5.md) | 4908 | `e9c995e77b981a71db33b85106c7a2f76c150600` |
| [`QA_ARCHITECTURE_B6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_ARCHITECTURE_B6.md) | 6812 | `677fedbaa18d0c774dd594f4152f471279610f63` |
| [`QA_BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md) | 5766 | `6e5fac66494b2575900eda0959cdac4acc4babdb` |
| [`QA_BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md) | 4090 | `d359e7acde542eaabab98bcba7fb662f6ed72679` |
| [`QA_BACKUP_2B43_3_SECURE_KEY_STORE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_BACKUP_2B43_3_SECURE_KEY_STORE.md) | 10964 | `ab950a195969b36002b1c97c61edc18607e1b857` |
| [`QA_DIOXUS_0_7_10_BASELINE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_DIOXUS_0_7_10_BASELINE.md) | 1056 | `f3ea545b7254a65ab5c17868755b52e4c10404a2` |
| [`QA_IMAGE_PASTE_STAGING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_IMAGE_PASTE_STAGING.md) | 4277 | `0f1a7b8614bdba0fa96e3de9cf22f9d25ad119bb` |
| [`QA_LIST_BACKSPACE_NATIVE_LIFT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_BACKSPACE_NATIVE_LIFT.md) | 3107 | `a63aa79c8a184f7a532cc4be91d75384b72ba87f` |
| [`QA_LIST_BOUNDARY_CONTINUITY.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_BOUNDARY_CONTINUITY.md) | 3307 | `7f6529bdbf62e5ff3cde4f9604943e027db8cf03` |
| [`QA_LIST_EXIT_INDENT_RESET.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_EXIT_INDENT_RESET.md) | 2899 | `de45e809e29f2f600ad9873f4e82976ed6dcecf3` |
| [`QA_LIST_NATIVE_SIMPLIFICATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_LIST_NATIVE_SIMPLIFICATION.md) | 3666 | `7562d9f4ee460cca70a12b34b576dd6db7478a0b` |
| [`QA_MACOS_APP_PACKAGING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_MACOS_APP_PACKAGING.md) | 2113 | `01d2aa2819b8f506b66d0be20a9091656011f18d` |
| [`QA_NATIVE_COMPILE_2B13.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B13.md) | 1564 | `ce43eb46c63dabe5cc517fcdb26029fb623da82e` |
| [`QA_NATIVE_COMPILE_2B14.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B14.md) | 1122 | `6c236cc87e491e93110c38072f556e71a07e4f5c` |
| [`QA_NATIVE_COMPILE_2B15.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B15.md) | 1492 | `6c68fb5469d3727011b63c4a9236701cce354a42` |
| [`QA_NATIVE_COMPILE_2B16.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B16.md) | 586 | `1e4c5f50046f106bc7af6357078e1f591697428c` |
| [`QA_NATIVE_COMPILE_2B17.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B17.md) | 809 | `753b887682be015ea7fd0f4d07acccb8826e32da` |
| [`QA_NATIVE_COMPILE_2B18.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B18.md) | 1540 | `e8ed79aec26b1ea82e6a27251377febb4da732ed` |
| [`QA_NATIVE_COMPILE_2B19.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B19.md) | 1653 | `9fe31b6e680026f036ceff4374cb594445811ae4` |
| [`QA_NATIVE_COMPILE_2B20.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B20.md) | 2086 | `927235dd186d0fe459fd2288ba33f8a0c1080c65` |
| [`QA_NATIVE_COMPILE_2B21.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B21.md) | 1579 | `7b3be70d357cef509febaffb829d45ab063ea89e` |
| [`QA_NATIVE_COMPILE_2B22.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B22.md) | 1299 | `b75fefa4ca7c69699d8edc8600271a345a5624d8` |
| [`QA_NATIVE_COMPILE_2B23.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B23.md) | 1393 | `714c2766d8abd3e3f6e9eda11fea77399ba21316` |
| [`QA_NATIVE_COMPILE_2B24.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B24.md) | 2005 | `9dde75e31c74cf517f5c64d3922f52b5dcaba882` |
| [`QA_NATIVE_COMPILE_2B25.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B25.md) | 1414 | `d0338d3cc231dc9b2d92927b3c43e2f4f574574a` |
| [`QA_NATIVE_COMPILE_2B26.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B26.md) | 1521 | `6d2e4196dc390acf7ff0925a47c5b8d4ebc05aee` |
| [`QA_NATIVE_COMPILE_2B27.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B27.md) | 1174 | `40653e369fb3b8e404bfcbd4c7c13ef64b990741` |
| [`QA_NATIVE_COMPILE_2B28.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B28.md) | 936 | `4deb76c8573f36b909318f55d7931cb4803a3558` |
| [`QA_NATIVE_COMPILE_2B29.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B29.md) | 1643 | `e770fa97a88b9eb659802e000af3d929966ce39f` |
| [`QA_NATIVE_COMPILE_2B30.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B30.md) | 262 | `486e6bfb5ee4a4256f2f941445808dd0311667d1` |
| [`QA_NATIVE_COMPILE_2B31.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B31.md) | 3332 | `41e0ed06cdb24aa5084eb444d80c13027b06331c` |
| [`QA_NATIVE_COMPILE_2B32.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B32.md) | 2208 | `24f8ba7da55efd5eac479f571110376c92cdcbc6` |
| [`QA_NATIVE_COMPILE_2B33.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B33.md) | 548 | `378c48f37889a206e3d8ccd0914012ac3fae9226` |
| [`QA_NATIVE_COMPILE_2B34.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B34.md) | 796 | `a2652242a9600db8b608133794d7540f92bdd541` |
| [`QA_NATIVE_COMPILE_2B35.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B35.md) | 980 | `f5267fcae44bce50517e4c5974b502e27d99dd28` |
| [`QA_NATIVE_COMPILE_2B36.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B36.md) | 569 | `7e924df318ba73238cb23a063eeb8dd5d249ffc1` |
| [`QA_NATIVE_COMPILE_2B37.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B37.md) | 462 | `94af7b6399c5741ff41ff8fe6ea6b9d26c5fcb80` |
| [`QA_NATIVE_COMPILE_2B38.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B38.md) | 790 | `52379e40ceb7d278ce0c80046bc66d8e8a933f73` |
| [`QA_NATIVE_COMPILE_2B39.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B39.md) | 724 | `7ba59122ff0d83fe05e60bbc14faba393a418fd6` |
| [`QA_NATIVE_COMPILE_2B40.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B40.md) | 1307 | `748a8e74385011bc78808bef842e991db7f9a075` |
| [`QA_NATIVE_COMPILE_2B41.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_NATIVE_COMPILE_2B41.md) | 918 | `efd1f17ed1e92d15c5374df76372336647068df7` |
| [`QA_PARAGRAPH_UI_REFINEMENT.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_PARAGRAPH_UI_REFINEMENT.md) | 3290 | `7d276f0a55a0e267c994de5058aaa62f02723b7f` |
| [`QA_PDF_RUNTIME_INTEGRITY.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_PDF_RUNTIME_INTEGRITY.md) | 2645 | `c51ac64d7aa313989a7906710637446e6d3295e7` |
| [`QA_PDF_VIEWER_UI_RESTORATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_PDF_VIEWER_UI_RESTORATION.md) | 2756 | `6740601d72ed6be34f4d6622c6c82597cd1840ad` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A10.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A10.md) | 3950 | `dc4ff0e270839eb88541896f08bbb9fb93043507` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A11.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A11.md) | 3014 | `d8cec2c03af9d101c791c0559f4376fb13eabc31` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A12.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A12.md) | 2124 | `22b949e00de13f9b87a69fbb5010e665960220b2` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A13.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A13.md) | 2581 | `86db2ae35b12f884656d7ec6e7828e8e2eb05f6c` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A14.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A14.md) | 2870 | `804c9c5bda13927dbd96da3dd97b527b8f5e3382` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A19.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A19.md) | 6908 | `3b932d83d4cfc21aa14ee7313606dc7bf46e9c39` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A6.md) | 5982 | `7d0dd92f8d450017ca146d61ecfe3b2cf752b25f` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A7.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A7.md) | 3320 | `8362560d1ec4c1baf7e621ad055c5a81aefc41bf` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A8.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A8.md) | 3637 | `d08d04a845f46eb51be94cdf0e5f4743cb070c37` |
| [`QA_SECURITY_E0_E3_CLOSURE_2A9.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_CLOSURE_2A9.md) | 3949 | `f4118cc4565c0f4464845fb44056a0f91aff7d14` |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A15.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A15.md) | 2577 | `ec1f981978a884956073935195762955e7622858` |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A16.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A16.md) | 3497 | `733a165f1d826bc18aa95b6c5fb71b0b2afb17a5` |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A17.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A17.md) | 4978 | `b042e4f833eb725fe98c64b1023ce7d1f51d7a73` |
| [`QA_SECURITY_E0_E3_REMEDIATION_2A18.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0_E3_REMEDIATION_2A18.md) | 4958 | `d06f4f4258a83631ddb1a9d6e726d05f426650a6` |
| [`QA_SECURITY_E0.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E0.md) | 3046 | `5124f34282e58ba65b73f909ed6bfa7fac9fd2b7` |
| [`QA_SECURITY_E1.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E1.md) | 2685 | `2cd237ac27eafda2d16a5ba19c48f523e44027ab` |
| [`QA_SECURITY_E2.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E2.md) | 3988 | `7237450863a730e3616929bb03dccadbf6ce7665` |
| [`QA_SECURITY_E3.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_SECURITY_E3.md) | 6642 | `060053fcbd323956459d6fd5e3e9dae8ff3388bd` |
| [`QA_VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md) | 4210 | `14d85c88b38d80fc1e9d2724bcf46718104919ea` |
| [`QA_VAULT_2B1_FOUNDATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B1_FOUNDATION.md) | 5157 | `3852575a1407562bd4063e81f67f685808dd1de8` |
| [`QA_VAULT_2B10_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B10_CLOSURE.md) | 3756 | `737a82eb4d749b8c6de8e2a882cad9b85f3624e8` |
| [`QA_VAULT_2B11_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B11_CLOSURE.md) | 3818 | `ef2118cf2052e16e424407e59f62f1f3a3eb9c90` |
| [`QA_VAULT_2B12_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B12_CLOSURE.md) | 2725 | `fd1eb9dd852173d37f96081a825ad64d91f7d74b` |
| [`QA_VAULT_2B2_PRIVACY_AUTHORITY.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B2_PRIVACY_AUTHORITY.md) | 6523 | `20b583ae3b33bc8633f174d1f605084a576df373` |
| [`QA_VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md) | 3170 | `29a5c98291a3359b49aa0dc5da3cb6e7014a4cd1` |
| [`QA_VAULT_2B3_PUBLICATION_TOKEN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B3_PUBLICATION_TOKEN.md) | 6996 | `1b6ef7b72698d3d179840f7b40e4222b08a0a149` |
| [`QA_VAULT_2B4_GRANULAR_CUTOVER.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B4_GRANULAR_CUTOVER.md) | 4685 | `db05150072779761d3fc1a5d1783f77dae185f41` |
| [`QA_VAULT_2B42_1_6_POST_AUDIT_REPAIR.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_1_6_POST_AUDIT_REPAIR.md) | 5683 | `b0dee2750d22232c638d51569370bb65d6f29873` |
| [`QA_VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md) | 3462 | `6308e6585c9eebea0a51d7a68a0194b8dcf9c487` |
| [`QA_VAULT_2B42_10_INDEPENDENT_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_10_INDEPENDENT_CLOSURE.md) | 4788 | `040dd054bc77e58dd812ced6a3ce1f39d30725d0` |
| [`QA_VAULT_2B42_11_FINAL_QA_PACKAGE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_11_FINAL_QA_PACKAGE.md) | 3198 | `17e515c1ca831283489189a1b90a587f68c8a0ec` |
| [`QA_VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md) | 2998 | `990f03b20280656312a255d004229d6353923d56` |
| [`QA_VAULT_2B42_3_NOTE_LOCAL_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_3_NOTE_LOCAL_UI.md) | 3051 | `f7b3dc46745dd56bd659c1cf0552980cb9ce2991` |
| [`QA_VAULT_2B42_4_MULTI_NOTE_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_4_MULTI_NOTE_UI.md) | 4625 | `eab16533042fab5ec1102f8912ef690a752cf242` |
| [`QA_VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md) | 6515 | `e9457fd105194d82b3beaf2789abce20370ec323` |
| [`QA_VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md) | 5439 | `43f960c4fb8871c5251fba118ed341701d8e1118` |
| [`QA_VAULT_2B42_7_NATIVE_COMPILE_FIX.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_7_NATIVE_COMPILE_FIX.md) | 2156 | `9426c0d2de60e87d36127d02655808b73c2dda71` |
| [`QA_VAULT_2B42_7_PROFILING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_7_PROFILING.md) | 3872 | `6f31fd71bc0379a890b52735f1a3d3bd09283037` |
| [`QA_VAULT_2B42_8_NO_VACUUM_FORENSIC.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_8_NO_VACUUM_FORENSIC.md) | 3276 | `0a60f9575d326d695a2d1c4b5bfd560ac38f3e20` |
| [`QA_VAULT_2B42_9_CLEANUP.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B42_9_CLEANUP.md) | 4844 | `28f7a74903d8040db640196c8bd969330b66b49f` |
| [`QA_VAULT_2B5_CONSOLIDATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B5_CONSOLIDATION.md) | 7765 | `dabf7c413ca46b97fa6116541db6afb382c40126` |
| [`QA_VAULT_2B6_INDEPENDENT_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B6_INDEPENDENT_CLOSURE.md) | 9277 | `59cb0f6f72cd895bb038a2f9e4a6f8f07a1d6ca9` |
| [`QA_VAULT_2B7_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B7_CLOSURE.md) | 8532 | `8fa65b740c6cceea6278693e5d80c49e5aedc245` |
| [`QA_VAULT_2B8_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B8_CLOSURE.md) | 6026 | `949f4f04f792a2cfeaa37570ce0a62241607273b` |
| [`QA_VAULT_2B9_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VAULT_2B9_CLOSURE.md) | 3932 | `a166e227ce3090850f5bd775a8458766e2022bab` |
| [`QA_VERSION_V0.12.6_BUILD_2B43.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/QA_VERSION_V0.12.6_BUILD_2B43.md) | 819 | `b23db46071d3e4161501b92c1c0b854b4dc23082` |

### Native compile implementation and plan history (58)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`NATIVE_COMPILE_2B13_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B13_IMPLEMENTATION.md) | 2802 | `43beabc2582662f271c6b648f7c57f444a3130ee` |
| [`NATIVE_COMPILE_2B13_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B13_PLAN.md) | 1604 | `8f13dbc8df9e70d0418ddece490f5b762ae2ef53` |
| [`NATIVE_COMPILE_2B14_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B14_IMPLEMENTATION.md) | 1107 | `acc5d2100153b5a87f7b19abc7f5389e24a72b53` |
| [`NATIVE_COMPILE_2B14_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B14_PLAN.md) | 1266 | `67a87e412320a99351d71d6dde34ae35dd4ab414` |
| [`NATIVE_COMPILE_2B15_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B15_IMPLEMENTATION.md) | 2628 | `eb6bc5b79b6ea707ce60259a81551ff8d4f96903` |
| [`NATIVE_COMPILE_2B15_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B15_PLAN.md) | 1251 | `6452bf7fe2c5652912269098b1712b2232217e64` |
| [`NATIVE_COMPILE_2B16_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B16_IMPLEMENTATION.md) | 636 | `76e41fa0e679b96527a78d537e67eee32f408c03` |
| [`NATIVE_COMPILE_2B16_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B16_PLAN.md) | 1049 | `5b9749234147c5f103d58e2ca0a0b142ae61f867` |
| [`NATIVE_COMPILE_2B17_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B17_IMPLEMENTATION.md) | 896 | `3ec7461428b8edcd49f1c6e75610b3dab8922857` |
| [`NATIVE_COMPILE_2B17_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B17_PLAN.md) | 955 | `fe24297697c3c8e99f5f6de742f184e4f23cda53` |
| [`NATIVE_COMPILE_2B18_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B18_IMPLEMENTATION.md) | 1435 | `d37951a50b68cd42e866eafe69ac484815be86d5` |
| [`NATIVE_COMPILE_2B18_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B18_PLAN.md) | 1205 | `cedbe315dbe129062e6573c7b8e6d819e7bc3fd5` |
| [`NATIVE_COMPILE_2B19_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B19_IMPLEMENTATION.md) | 928 | `fa8b2c7dd5cded2cd240bdbb2d9d000adf8efee1` |
| [`NATIVE_COMPILE_2B19_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B19_PLAN.md) | 1254 | `7bd330776e8fb14fc04a5d8b32f662b0785ef9fa` |
| [`NATIVE_COMPILE_2B20_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B20_IMPLEMENTATION.md) | 2929 | `d1f85ae5a1434750ff85ab8518d9767ad3dbc006` |
| [`NATIVE_COMPILE_2B20_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B20_PLAN.md) | 2644 | `814a26bbe63d669f25fec95dddde6d7920d228be` |
| [`NATIVE_COMPILE_2B21_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B21_IMPLEMENTATION.md) | 2578 | `0ad67edf7e91aabc349dfb444d3131821c0b00e8` |
| [`NATIVE_COMPILE_2B21_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B21_PLAN.md) | 2992 | `9f97d42d96b53193968f51840583c530ca9ecb19` |
| [`NATIVE_COMPILE_2B22_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B22_IMPLEMENTATION.md) | 1250 | `8dcf2b75519b4dce7069cffa8b025037a693916f` |
| [`NATIVE_COMPILE_2B22_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B22_PLAN.md) | 1392 | `91beab058b0e1376ec8988c230f0e95d8083fc3e` |
| [`NATIVE_COMPILE_2B23_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B23_IMPLEMENTATION.md) | 1085 | `28485a2e0f2724c6dadfc1dc5d6c5e7694b16e96` |
| [`NATIVE_COMPILE_2B23_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B23_PLAN.md) | 1217 | `85a8573460ec643867c53374e2f1fc45a15e919c` |
| [`NATIVE_COMPILE_2B24_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B24_IMPLEMENTATION.md) | 1655 | `25919e422d71b79a88b1217ba80a7a883743202c` |
| [`NATIVE_COMPILE_2B24_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B24_PLAN.md) | 1974 | `f7e1f940a5f8f2044d7237eaca9efeb64d20285e` |
| [`NATIVE_COMPILE_2B25_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B25_IMPLEMENTATION.md) | 877 | `da8539a138c624006da62daa476d410c6727e49d` |
| [`NATIVE_COMPILE_2B25_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B25_PLAN.md) | 1158 | `dcf43de26d7abe90ed71fb9213dbfa185de8d39e` |
| [`NATIVE_COMPILE_2B26_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B26_IMPLEMENTATION.md) | 1388 | `2ce2692380030ec37464c44e6644c3b17b938122` |
| [`NATIVE_COMPILE_2B26_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B26_PLAN.md) | 1395 | `f0286a84b0ce4c094adf39fc2fb6bf92aabf3758` |
| [`NATIVE_COMPILE_2B27_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B27_IMPLEMENTATION.md) | 1223 | `724d7d6a7a2dd415a7edc43663048550b5655b96` |
| [`NATIVE_COMPILE_2B27_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B27_PLAN.md) | 1435 | `a0f2d0b186f68dae0e58673316e6d5b95a9ec891` |
| [`NATIVE_COMPILE_2B28_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B28_IMPLEMENTATION.md) | 744 | `73932e7555925b7d70958d043965bf829030ccab` |
| [`NATIVE_COMPILE_2B28_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B28_PLAN.md) | 1026 | `554c49b3aa10ececad151e84f5f06051285a5ab3` |
| [`NATIVE_COMPILE_2B29_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B29_IMPLEMENTATION.md) | 1281 | `aa6d471ca36d1f6d6d73500a588f86d63bf06a8c` |
| [`NATIVE_COMPILE_2B29_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B29_PLAN.md) | 1073 | `6ad3d79c7364397791ee3ac6f7ac25fb727e797d` |
| [`NATIVE_COMPILE_2B30_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B30_IMPLEMENTATION.md) | 499 | `0d992e7737d0a9aabf0fc4d28e7163c9f699847b` |
| [`NATIVE_COMPILE_2B30_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B30_PLAN.md) | 527 | `11d4e9d2aec5dba3eade1a762c2a59873b9d87da` |
| [`NATIVE_COMPILE_2B31_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B31_IMPLEMENTATION.md) | 2188 | `1d987faf5de47f956253d6b41a6b164fa5c525bd` |
| [`NATIVE_COMPILE_2B31_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B31_PLAN.md) | 2037 | `1bc1ca2eee7ef8b73aad6c2b07ce65124f299d1b` |
| [`NATIVE_COMPILE_2B32_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B32_IMPLEMENTATION.md) | 1114 | `f9cef87e9fa4a2c7a1da7859e2d650465825cc27` |
| [`NATIVE_COMPILE_2B32_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B32_PLAN.md) | 1122 | `538e6c8f6b9b7d9af6d2f4f48a3a83d7ddf6601a` |
| [`NATIVE_COMPILE_2B33_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B33_IMPLEMENTATION.md) | 559 | `bccf12b71660eaec59afc1a4f6e0288a1ecbb15f` |
| [`NATIVE_COMPILE_2B33_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B33_PLAN.md) | 917 | `ca815776fce15686585d5afe507efc42d80ab4dd` |
| [`NATIVE_COMPILE_2B34_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B34_IMPLEMENTATION.md) | 777 | `ccef49bbd1952a239d23f6a80dbfe4d420f203aa` |
| [`NATIVE_COMPILE_2B34_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B34_PLAN.md) | 1156 | `36fadf9bde7c17d0f963c51ff644feaeed4cc3bf` |
| [`NATIVE_COMPILE_2B35_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B35_IMPLEMENTATION.md) | 1473 | `3c0c02b6ae5bd0d694ad3e092eb52fe9b7df15ac` |
| [`NATIVE_COMPILE_2B35_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B35_PLAN.md) | 2328 | `8615d8107942e8f0e6a74b33e42a0dd89fea140e` |
| [`NATIVE_COMPILE_2B36_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B36_IMPLEMENTATION.md) | 916 | `2538bcb94de4fc260a3d49e4fce9c24a609f14d6` |
| [`NATIVE_COMPILE_2B36_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B36_PLAN.md) | 670 | `0311eefd0967cf0873ffc7e48245e6c44643eb7f` |
| [`NATIVE_COMPILE_2B37_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B37_IMPLEMENTATION.md) | 671 | `5f3e523f563fc27bdcc341e7a0fb23c76c26bd3e` |
| [`NATIVE_COMPILE_2B37_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B37_PLAN.md) | 776 | `ca17ab8a61aefbb49879c992eb49f0be6b60d6aa` |
| [`NATIVE_COMPILE_2B38_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B38_IMPLEMENTATION.md) | 1120 | `5e351129fa3801e2a36b29b94482aaced1b35742` |
| [`NATIVE_COMPILE_2B38_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B38_PLAN.md) | 1329 | `920770ae7be5f55b7f9ba0d7292e79734b944ed2` |
| [`NATIVE_COMPILE_2B39_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B39_IMPLEMENTATION.md) | 753 | `ac939d532ff0e7dc52b5acb58d855ba74b3a1301` |
| [`NATIVE_COMPILE_2B39_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B39_PLAN.md) | 549 | `1bc1de864073cbb2f221b2013f93533828f3f755` |
| [`NATIVE_COMPILE_2B40_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B40_IMPLEMENTATION.md) | 1505 | `f1755065994cb1fb77cd32e93e4f8ac843507705` |
| [`NATIVE_COMPILE_2B40_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B40_PLAN.md) | 1489 | `181d67b9cc9d56e4ce97cbd1c3cc7b53633d451c` |
| [`NATIVE_COMPILE_2B41_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B41_IMPLEMENTATION.md) | 813 | `c58c2aa1682bd3d6eb1f7b94a6f31461f45eb58c` |
| [`NATIVE_COMPILE_2B41_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B41_PLAN.md) | 938 | `65e41b9332ab90ebde2a88aa6d78feea548768eb` |

### Version and promotion history (1)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`VERSION_PROMOTION_V0.12.6_BUILD_2B43.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VERSION_PROMOTION_V0.12.6_BUILD_2B43.md) | 1255 | `9f15f48312e811266b4d5ff14f1b0f28ab16a6f1` |

### Component guides, templates and bundled content (7)

| Document (original path) | Bytes | Git blob SHA |
| --- | ---: | --- |
| [`.github/ISSUE_TEMPLATE/bug_report.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/.github/ISSUE_TEMPLATE/bug_report.md) | 455 | `52ccfc2fe7de6c34972a28991eda4fbd6b89806c` |
| [`.github/pull_request_template.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/.github/pull_request_template.md) | 967 | `6e1440419fe5c86c9b6a62646f29225bc432d718` |
| [`assets/welcome-note.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/assets/welcome-note.md) | 332 | `9a21007c2a64e7ee03e9e946bc29744e05cc59c3` |
| [`editor-spec/README.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/editor-spec/README.md) | 3410 | `800b36650b1f2ad63a085fb49bfe6f53f6a446c1` |
| [`editor-src/README.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/editor-src/README.md) | 1753 | `2797085710d40bc46ff1029f12d20299370a97dc` |
| [`pdf-src/README.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/pdf-src/README.md) | 2244 | `5f537db1a6fc8a687f2bc79f819e34e2f7aea91a` |
| [`tools/e0-crypto-bench/README.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/tools/e0-crypto-bench/README.md) | 861 | `6c3a5b053fcec5c1ffd83418857429c8cd9e9469` |

## Verification and updating

- The inventory is **frozen to the main baseline commit**, not a live list of branch changes; record later additions separately until consolidation closes.
- Before merging a replacement, inspect its exact content, identify governing versions and later amendments, and map every preserved requirement.
- Check references made by CI, shell/JS validators, Rust source, root documentation, and contributor instructions before renaming any historical document.
- Cross-check file count, original paths and blob SHAs against the frozen baseline. Existing documents should not change during Batch 0.
