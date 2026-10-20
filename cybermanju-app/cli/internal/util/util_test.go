// CyberManju OS — Util test (cli/internal/util/util_test.go).
// Role: CLI utilities.
// Exports: TestParseSize, TestFormatBytes, TestCompareVersions, TestAssetName.
// Integrates: testing, 512, 512M, 10G, 1.5G, 2T.
// Contracts: none special (pure logic; errors bubble to caller).
package util

import "testing"

func TestParseSize(t *testing.T) {
	for in, want := range map[string]uint64{
		"512":  512,
		"512M": 512 << 20,
		"10G":  10 << 30,
		"1.5G": uint64(1.5 * (1 << 30)),
		"2T":   2 << 40,
		"64K":  64 << 10,
		" 8g ": 8 << 30,
	} {
		got, err := ParseSize(in)
		if err != nil || got != want {
			t.Fatalf("ParseSize(%q) = %d, %v (want %d)", in, got, err, want)
		}
	}
	for _, bad := range []string{"", "abc", "0", "-5G", "10X"} {
		if _, err := ParseSize(bad); err == nil {
			t.Fatalf("ParseSize(%q) should fail", bad)
		}
	}
}

func TestFormatBytes(t *testing.T) {
	if got := FormatBytes(512); got != "512B" {
		t.Fatalf("got %s", got)
	}
	if got := FormatBytes(10 << 30); got != "10.0GB" {
		t.Fatalf("got %s", got)
	}
}

func TestCompareVersions(t *testing.T) {
	if CompareVersions("v0.2.0", "v0.1.0") <= 0 {
		t.Fatal("0.2.0 > 0.1.0")
	}
	if CompareVersions("v0.1.0", "v0.1.0") != 0 {
		t.Fatal("equal")
	}
	if CompareVersions("dev", "v0.1.0") >= 0 {
		t.Fatal("dev sorts below releases")
	}
}

func TestAssetName(t *testing.T) {
	if got := AssetName("linux", "amd64"); got != "cyb-linux-amd64.tar.gz" {
		t.Fatalf("got %s", got)
	}
	if got := AssetName("windows", "arm64"); got != "cyb-windows-arm64.tar.gz" {
		t.Fatalf("got %s", got)
	}
	if got := AssetName("android", "arm64"); got != "cyb-android-arm64.tar.gz" {
		t.Fatalf("got %s", got)
	}
}
