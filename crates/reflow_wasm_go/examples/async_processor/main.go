package main

import (
	reflow "github.com/darmie/reflow/reflow_wasm_go/sdk"
)

func processAsyncProcessor(context reflow.ActorContext) (reflow.ActorResult, error) {
	outputs := make(map[string]reflow.Message)

	// Get configuration
	chunkSize := int64(10)
	if val, ok := context.Config.GetInt("chunk_size"); ok {
		chunkSize = val
	}

	sendProgress := true
	if val, ok := context.Config.GetBool("send_progress"); ok {
		sendProgress = val
	}

	// Check if we have data to process
	if dataMsg, exists := context.Payload["data"]; exists && dataMsg.Type == "Array" {
		data, ok := dataMsg.Data.([]interface{})
		if !ok {
			// data must be an array
			return reflow.ActorResult{}, nil
		}

		// Send initial status asynchronously
		reflow.SendOutput(map[string]interface{}{
			"status": "Starting processing",
		})

		// Process data in chunks
		totalItems := len(data)
		processedCount := 0
		var results []float64

		for chunkIdx := 0; chunkIdx < len(data); chunkIdx += int(chunkSize) {
			end := chunkIdx + int(chunkSize)
			if end > len(data) {
				end = len(data)
			}

			chunk := data[chunkIdx:end]

			// Process chunk
			for _, item := range chunk {
				var num float64
				switch v := item.(type) {
				case float64:
					num = v
				case int:
					num = float64(v)
				default:
					continue
				}

				// Simulate some processing (double the number to match test expectations)
				results = append(results, num*2)
				processedCount++
			}

			// Send progress update asynchronously
			if sendProgress {
				progress := int64(processedCount * 100 / totalItems)
				reflow.SendOutput(map[string]interface{}{
					"progress": progress,
					"status":   "Processing chunk",
				})
			}

			// Update state with progress
			reflow.SetState("last_chunk_processed", chunkIdx/int(chunkSize))
			reflow.SetState("items_processed", processedCount)
		}

		// Send completion status
		reflow.SendOutput(map[string]interface{}{
			"status": "Processing complete",
		})

		// Calculate average
		var average float64
		if len(results) > 0 {
			sum := 0.0
			for _, val := range results {
				sum += val
			}
			average = sum / float64(len(results))
		}

		// Return final result
		resultData := map[string]interface{}{
			"processed_count": processedCount,
			"results":         results,
			"average":         average,
		}

		outputs["result"] = reflow.NewObjectMessage(resultData)

	} else if _, exists := context.Payload["start"]; exists {
		// Just starting - check if we have previous state
		lastChunk, _ := reflow.GetState("last_chunk_processed")

		var statusMsg string
		if lastChunk != nil {
			lastChunkVal := int64(-1)

			if val, ok := lastChunk.(float64); ok {
				lastChunkVal = int64(val)
			}

			if lastChunkVal >= 0 {
				statusMsg = "Ready to process. Previous run completed."
			} else {
				statusMsg = "Ready to process. No previous runs."
			}
		} else {
			statusMsg = "Ready to process. No previous runs."
		}

		outputs["status"] = reflow.NewStringMessage(statusMsg)
	}

	// Convert Message outputs to interface{} for ActorResult
	plainOutputs := make(map[string]interface{})
	for k, msg := range outputs {
		plainOutputs[k] = msg.ToSerializable()
	}
	
	return reflow.ActorResult{
		Outputs: plainOutputs,
		State:   nil, // State managed via host functions
	}, nil
}

func getAsyncProcessorMetadata() reflow.PluginMetadata {
	configSchema := map[string]interface{}{
		"type": "object",
		"properties": map[string]interface{}{
			"chunk_size": map[string]interface{}{
				"type":        "integer",
				"description": "Size of chunks to process",
				"default":     10,
			},
			"send_progress": map[string]interface{}{
				"type":        "boolean",
				"description": "Whether to send progress updates",
				"default":     true,
			},
		},
	}

	return reflow.PluginMetadata{
		Component:   "GoAsyncProcessor",
		Description: "Demonstrates asynchronous processing using host functions",
		Inports: []reflow.PortDefinition{
			reflow.NewOptionalPort("start", "Start processing", "Flow"),
			reflow.NewOptionalPort("data", "Data to process", "Array"),
		},
		Outports: []reflow.PortDefinition{
			reflow.NewOptionalPort("status", "Processing status", "String"),
			reflow.NewOptionalPort("progress", "Progress updates", "Integer"),
			reflow.NewOptionalPort("result", "Final result", "Object"),
		},
		ConfigSchema: configSchema,
	}
}

func init() {
	reflow.RegisterPlugin(getAsyncProcessorMetadata(), processAsyncProcessor)
}

// Export functions required by Extism (use //go:wasmexport for Go 1.21+)
//export get_metadata
func get_metadata() int32 {
	return reflow.GetMetadata()
}

//export process
func process() int32 {
	return reflow.Process()
}

func main() {}
