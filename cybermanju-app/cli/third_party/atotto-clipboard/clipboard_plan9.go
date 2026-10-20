// CyberManju OS — Plan 9 clipboard bridge (cli/third_party/atotto-clipboard/clipboard_plan9.go).
// Role: Preserve the upstream /dev/snarf clipboard behavior in the local fork.
// Exports: readAll, writeAll.
// Integrates: os, io/ioutil.
// Contracts: BSD-licensed upstream implementation; see LICENSE.
// Copyright 2013 @atotto. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//go:build plan9
// +build plan9

package clipboard

import (
	"io/ioutil"
	"os"
)

func readAll() (string, error) {
	f, err := os.Open("/dev/snarf")
	if err != nil {
		return "", err
	}
	defer f.Close()

	str, err := ioutil.ReadAll(f)
	if err != nil {
		return "", err
	}

	return string(str), nil
}

func writeAll(text string) error {
	f, err := os.OpenFile("/dev/snarf", os.O_WRONLY, 0666)
	if err != nil {
		return err
	}
	defer f.Close()

	_, err = f.Write([]byte(text))
	if err != nil {
		return err
	}

	return nil
}
