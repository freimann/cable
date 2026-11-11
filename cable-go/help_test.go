package main

import "testing"

func TestEmbeddedHelpIsTheRecoveredFile(t *testing.T) {
	if len(helpLines) == 0 || !contains(helpLines[0], "Version 1.40") {
		t.Fatalf("first help line = %q", helpLines[0])
	}
	if !anyLineContains("End of Help") {
		t.Fatal("help has no end marker")
	}
	if !anyLineContains("CABLE-RS") {
		t.Fatal("help has no CABLE-RS note")
	}
	if !anyLineContains("F8") {
		t.Fatal("help has no F8 note")
	}
	for _, line := range helpLines {
		if contains(line, "\r") {
			t.Fatalf("help line contains CR: %q", line)
		}
	}
}

func contains(s, sub string) bool {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}

func anyLineContains(sub string) bool {
	for _, line := range helpLines {
		if contains(line, sub) {
			return true
		}
	}
	return false
}
