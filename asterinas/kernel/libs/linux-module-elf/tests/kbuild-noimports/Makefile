# SPDX-License-Identifier: GPL-2.0-only
obj-m += cybermanju_noimports_fixture.o
# These flags apply only to this fixture object; no kernel-wide/default claim.
CFLAGS_REMOVE_cybermanju_noimports_fixture.o += -pg -mfentry -mfunction-return=thunk-extern
CFLAGS_cybermanju_noimports_fixture.o += -fno-instrument-functions -mfunction-return=keep
# Test-only bypass of objtool return-thunk checks. NOT a validated mitigation
# configuration and NOT permission to load or execute this object.
OBJECT_FILES_NON_STANDARD_cybermanju_noimports_fixture.o := y
