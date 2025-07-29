package main

import (
	"encoding/json"
	"fmt"
	"encoding/base64"
)

type TestResult struct {
	Outputs map[string]interface{} `json:"outputs"`
	State   interface{}            `json:"state,omitempty"`
}

func main() {
	// Test what happens with different output methods
	result := TestResult{
		Outputs: map[string]interface{}{
			"status": map[string]interface{}{
				"type": "String",
				"data": "Ready to process. No previous runs.",
			},
		},
		State: nil,
	}
	
	data, err := json.Marshal(result)
	if err != nil {
		fmt.Printf("Marshal error: %v\n", err)
		return
	}
	
	fmt.Printf("JSON bytes: %s\n", string(data))
	fmt.Printf("JSON length: %d\n", len(data))
	
	// Simulate what PDK does
	fmt.Println("\nSimulating PDK behavior:")
	
	// Test AllocateBytes
	fmt.Printf("AllocateBytes would create memory with %d bytes\n", len(data))
	
	// Test what OutputJSON might do
	fmt.Println("\nIf OutputJSON base64 encodes:")
	encoded := base64.StdEncoding.EncodeToString(data)
	fmt.Printf("Base64: %s\n", encoded)
}