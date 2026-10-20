// CyberManju OS — CLI browser launcher (cli/cmd/open_url.go).
// Role: Open provider authorization URLs without PATH probing on Android.
// Exports: openURL.
// Integrates: github.com/pkg/browser, os/exec, runtime.
// Contracts: Android uses absolute /system/bin/am; other targets keep upstream behavior.
package cmd

import (
	"os/exec"
	"runtime"

	"github.com/pkg/browser"
)

func openURL(url string) error {
	if runtime.GOOS == "android" {
		return exec.Command("/system/bin/am", "start", "-a", "android.intent.action.VIEW", "-d", url).Run()
	}
	return browser.OpenURL(url)
}
