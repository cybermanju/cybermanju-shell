// CyberManju OS — CLI clipboard API (cli/third_party/atotto-clipboard/clipboard.go).
// Role: Preserve the upstream clipboard module interface used by Charm widgets.
// Exports: ReadAll, WriteAll, Unsupported.
// Integrates: Platform-specific clipboard helpers.
// Contracts: BSD-licensed upstream implementation; see LICENSE.
// Copyright 2013 @atotto. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

// Package clipboard read/write on clipboard
package clipboard

// ReadAll read string from clipboard
func ReadAll() (string, error) {
	return readAll()
}

// WriteAll write string to clipboard
func WriteAll(text string) error {
	return writeAll(text)
}

// Unsupported might be set true during clipboard init, to help callers decide
// whether or not to offer clipboard options.
var Unsupported bool
