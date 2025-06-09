# Graph Layout System

Reflow's layout system provides intelligent automatic positioning and manual positioning capabilities for graph nodes. The system supports multiple layout algorithms, custom positioning, and integration with visual editors.

## Automatic Layout

### Basic Auto-Layout

```rust
use reflow_network::graph::Position;

// Calculate optimal positions using default algorithm
let positions = graph.calculate_layout();

for (node_id, position) in positions {
    println!("Node {}: x={:.1}, y={:.1}", node_id, position.x, position.y);
}

// Apply calculated layout to graph metadata
graph.auto_layout()?;
```

### Layout Algorithms

The system supports multiple layout algorithms optimized for different graph types:

```rust
use reflow_network::graph::{LayoutAlgorithm, LayoutConfig};

// Hierarchical layout for DAGs (default)
let hierarchical_config = LayoutConfig {
    algorithm: LayoutAlgorithm::Hierarchical,
    node_spacing: 120.0,
    layer_spacing: 80.0,
    edge_spacing: 40.0,
    ..Default::default()
};

let positions = graph.calculate_layout_with_config(&hierarchical_config);

// Force-directed layout for general graphs
let force_config = LayoutConfig {
    algorithm: LayoutAlgorithm::ForceDirected,
    iterations: 100,
    spring_strength: 0.5,
    repulsion_strength: 1000.0,
    ..Default::default()
};

let positions = graph.calculate_layout_with_config(&force_config);

// Grid layout for structured workflows
let grid_config = LayoutConfig {
    algorithm: LayoutAlgorithm::Grid,
    grid_size: 150.0,
    columns: 5,
    align_to_grid: true,
    ..Default::default()
};

let positions = graph.calculate_layout_with_config(&grid_config);
```

### Hierarchical Layout

Best for directed acyclic graphs (DAGs) and workflow diagrams:

```rust
use reflow_network::graph::HierarchicalConfig;

let hierarchical = HierarchicalConfig {
    direction: LayoutDirection::TopToBottom,
    layer_spacing: 100.0,
    node_spacing: 80.0,
    edge_routing: EdgeRouting::Orthogonal,
    minimize_crossings: true,
    balance_nodes: true,
};

let positions = graph.hierarchical_layout(&hierarchical);

// Apply with automatic layer detection
graph.auto_layout_hierarchical()?;
```

### Force-Directed Layout

Ideal for general graphs with cycles and complex interconnections:

```rust
use reflow_network::graph::ForceDirectedConfig;

let force_config = ForceDirectedConfig {
    iterations: 150,
    cooling_factor: 0.95,
    initial_temperature: 100.0,
    spring_strength: 0.3,
    spring_length: 100.0,
    repulsion_strength: 800.0,
    gravity_strength: 0.1,
    node_charge: -30.0,
};

let positions = graph.force_directed_layout(&force_config);
```

### Organic Layout

Creates natural, flowing layouts:

```rust
use reflow_network::graph::OrganicConfig;

let organic_config = OrganicConfig {
    preferred_edge_length: 120.0,
    edge_length_cost_factor: 0.0001,
    node_distribution_cost_factor: 20000.0,
    edge_crossing_cost_factor: 6000.0,
    edge_distance_cost_factor: 15000.0,
    border_line_cost_factor: 100.0,
    max_iterations: 200,
};

let positions = graph.organic_layout(&organic_config);
```

## Manual Positioning

### Setting Node Positions

```rust
use reflow_network::graph::Position;

// Set specific position
graph.set_node_position("input_node", 0.0, 0.0)?;
graph.set_node_position("processor", 200.0, 100.0)?;
graph.set_node_position("output_node", 400.0, 0.0)?;

// Set position with custom anchor point
let position = Position {
    x: 150.0,
    y: 75.0,
    anchor: Some(Anchor { x: 0.5, y: 0.5 }), // Center anchor
};
graph.set_node_position_with_anchor("centered_node", position)?;
```

### Position Metadata Structure

Positions are stored in node metadata following this convention:

```rust
use serde_json::json;
use std::collections::HashMap;

// Standard position metadata
let position_metadata = HashMap::from([
    ("x".to_string(), json!(100)),
    ("y".to_string(), json!(150)),
    ("width".to_string(), json!(120)),
    ("height".to_string(), json!(80)),
    ("anchor".to_string(), json!({
        "x": 0.5,  // Horizontal anchor (0.0 = left, 0.5 = center, 1.0 = right)
        "y": 0.5   // Vertical anchor (0.0 = top, 0.5 = middle, 1.0 = bottom)
    }))
]);

graph.set_node_metadata("positioned_node", position_metadata);
```

### Retrieving Positions

```rust
// Get position for a specific node
if let Some(position) = graph.get_node_position("processor") {
    println!("Node position: ({}, {})", position.x, position.y);
}

// Get all node positions
let all_positions = graph.get_all_positions();
for (node_id, position) in all_positions {
    println!("{}: ({:.1}, {:.1})", node_id, position.x, position.y);
}

// Get positions within a bounding box
let bbox = BoundingBox {
    min_x: 0.0,
    min_y: 0.0,
    max_x: 500.0,
    max_y: 300.0,
};
let nodes_in_area = graph.get_nodes_in_area(bbox);
```

## Layout Constraints

### Alignment Constraints

```rust
use reflow_network::graph::{AlignmentConstraint, ConstraintType};

// Horizontal alignment
let horizontal_alignment = AlignmentConstraint {
    nodes: vec!["node1".to_string(), "node2".to_string(), "node3".to_string()],
    constraint_type: ConstraintType::HorizontalAlignment,
    offset: 0.0,
};

// Vertical alignment
let vertical_alignment = AlignmentConstraint {
    nodes: vec!["input1".to_string(), "input2".to_string()],
    constraint_type: ConstraintType::VerticalAlignment,
    offset: 50.0, // 50 pixels apart
};

// Apply constraints during layout
let config = LayoutConfig {
    algorithm: LayoutAlgorithm::Hierarchical,
    constraints: vec![horizontal_alignment, vertical_alignment],
    ..Default::default()
};

graph.apply_layout_with_constraints(&config)?;
```

### Distance Constraints

```rust
use reflow_network::graph::{DistanceConstraint, DistanceType};

// Minimum distance constraint
let min_distance = DistanceConstraint {
    from_node: "source".to_string(),
    to_node: "sink".to_string(),
    distance_type: DistanceType::Minimum,
    distance: 200.0,
};

// Maximum distance constraint
let max_distance = DistanceConstraint {
    from_node: "processor1".to_string(),
    to_node: "processor2".to_string(),
    distance_type: DistanceType::Maximum,
    distance: 300.0,
};

// Fixed distance constraint
let fixed_distance = DistanceConstraint {
    from_node: "controller".to_string(),
    to_node: "display".to_string(),
    distance_type: DistanceType::Fixed,
    distance: 150.0,
};
```

### Boundary Constraints

```rust
use reflow_network::graph::BoundaryConstraint;

// Keep nodes within bounds
let boundary = BoundaryConstraint {
    min_x: 0.0,
    min_y: 0.0,
    max_x: 1000.0,
    max_y: 600.0,
    enforce_during_layout: true,
};

// Apply boundary constraint
graph.set_layout_boundary(boundary);
```

## Layout Optimization

### Minimize Edge Crossings

```rust
// Optimize layout to reduce edge crossings
let optimized_positions = graph.minimize_edge_crossings()?;

// Apply optimization with maximum iterations
let crossings_config = EdgeCrossingConfig {
    max_iterations: 50,
    improvement_threshold: 0.01,
    use_barycenter_heuristic: true,
};

graph.optimize_edge_crossings(&crossings_config)?;
```

### Edge Bundling

```rust
use reflow_network::graph::EdgeBundling;

// Enable edge bundling for cleaner layouts
let bundling_config = EdgeBundling {
    enable: true,
    strength: 0.8,
    step_size: 0.1,
    iterations: 60,
    min_distance: 10.0,
};

graph.apply_edge_bundling(&bundling_config)?;
```

### Compact Layout

```rust
// Create compact layout by minimizing overall area
let compact_config = CompactLayoutConfig {
    preserve_aspect_ratio: true,
    min_node_spacing: 20.0,
    pack_components: true,
};

graph.create_compact_layout(&compact_config)?;
```

## Layer-Based Layout

### Automatic Layer Detection

```rust
use reflow_network::graph::{LayerAnalysis, LayerDirection};

// Detect natural layers in the graph
let layer_analysis = graph.analyze_layers();

println!("Detected {} layers:", layer_analysis.layers.len());
for (level, nodes) in layer_analysis.layers.iter().enumerate() {
    println!("  Layer {}: {:?}", level, nodes);
}

// Apply layer-based layout
let layer_config = LayerLayoutConfig {
    direction: LayerDirection::LeftToRight,
    layer_spacing: 150.0,
    node_spacing: 100.0,
    center_nodes_in_layer: true,
};

graph.apply_layer_layout(&layer_config)?;
```

### Manual Layer Assignment

```rust
// Manually assign nodes to layers
let layer_assignments = HashMap::from([
    ("input1".to_string(), 0),
    ("input2".to_string(), 0),
    ("processor1".to_string(), 1),
    ("processor2".to_string(), 1),
    ("output".to_string(), 2),
]);

graph.set_layer_assignments(layer_assignments);
graph.apply_layer_layout(&layer_config)?;
```

## Group-Based Layout

### Layout Node Groups

```rust
use reflow_network::graph::GroupLayoutConfig;

// Layout nodes within groups
let group_config = GroupLayoutConfig {
    group_spacing: 200.0,
    internal_spacing: 50.0,
    group_padding: 20.0,
    layout_algorithm: LayoutAlgorithm::Grid,
};

// Apply group-aware layout
graph.layout_groups(&group_config)?;

// Layout specific group
graph.layout_group("data_processing", &group_config)?;
```

### Group Boundaries

```rust
// Calculate group boundaries
let group_bounds = graph.calculate_group_bounds("data_processing");
if let Some(bounds) = group_bounds {
    println!("Group bounds: ({}, {}) to ({}, {})", 
        bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y);
}

// Set custom group boundary
let custom_bounds = BoundingBox {
    min_x: 100.0,
    min_y: 50.0,
    max_x: 400.0,
    max_y: 250.0,
};
graph.set_group_bounds("data_processing", custom_bounds);
```

## Advanced Layout Features

### Multi-Level Layout

For very large graphs, use multi-level layout:

```rust
use reflow_network::graph::MultiLevelConfig;

let multilevel_config = MultiLevelConfig {
    coarsening_factor: 0.7,
    max_levels: 5,
    uncoarsening_iterations: 10,
    finest_level_iterations: 20,
};

let positions = graph.multilevel_layout(&multilevel_config)?;
```

### Incremental Layout

Update layout incrementally when nodes are added/removed:

```rust
// Add node with incremental layout update
graph.add_node("new_processor", "DataProcessor", None);
graph.add_connection("source", "out", "new_processor", "in", None);

// Update layout incrementally
let incremental_config = IncrementalLayoutConfig {
    stabilization_iterations: 10,
    affected_nodes_only: true,
    preserve_existing_positions: true,
};

graph.incremental_layout_update(&incremental_config)?;
```

### Layout Animation Support

```rust
use reflow_network::graph::{LayoutAnimation, AnimationFrame};

// Generate animation frames for smooth transitions
let from_positions = graph.get_all_positions();
let to_positions = graph.calculate_layout();

let animation = LayoutAnimation::new(from_positions, to_positions, 30); // 30 frames

// Get animation frames
for (frame_idx, frame) in animation.frames().enumerate() {
    println!("Frame {}: {} position updates", frame_idx, frame.positions.len());
    
    // Apply frame in UI
    for (node_id, position) in frame.positions {
        // Update UI node position
        ui.set_node_position(&node_id, position.x, position.y);
    }
}
```

## Layout Quality Metrics

### Measuring Layout Quality

```rust
use reflow_network::graph::{LayoutMetrics, LayoutQuality};

let metrics = graph.calculate_layout_metrics();

println!("Layout Quality Metrics:");
println!("  Edge crossings: {}", metrics.edge_crossings);
println!("  Average edge length: {:.2}", metrics.average_edge_length);
println!("  Node distribution score: {:.2}", metrics.node_distribution_score);
println!("  Aspect ratio: {:.2}", metrics.aspect_ratio);
println!("  Overall score: {:.2}", metrics.overall_quality_score);

// Detailed metrics
println!("\nDetailed Metrics:");
println!("  Minimum edge length: {:.2}", metrics.min_edge_length);
println!("  Maximum edge length: {:.2}", metrics.max_edge_length);
println!("  Edge length variance: {:.2}", metrics.edge_length_variance);
println!("  Node overlap count: {}", metrics.node_overlaps);
println!("  Angular resolution: {:.2}°", metrics.angular_resolution);
```

### Layout Comparison

```rust
// Compare different layout algorithms
let algorithms = vec![
    LayoutAlgorithm::Hierarchical,
    LayoutAlgorithm::ForceDirected,
    LayoutAlgorithm::Organic,
];

let mut best_layout = None;
let mut best_score = 0.0;

for algorithm in algorithms {
    let config = LayoutConfig {
        algorithm: algorithm.clone(),
        ..Default::default()
    };
    
    let positions = graph.calculate_layout_with_config(&config);
    graph.apply_positions(positions);
    
    let metrics = graph.calculate_layout_metrics();
    let score = metrics.overall_quality_score;
    
    println!("{:?}: score {:.2}", algorithm, score);
    
    if score > best_score {
        best_score = score;
        best_layout = Some(algorithm);
    }
}

if let Some(best) = best_layout {
    println!("Best layout algorithm: {:?} (score: {:.2})", best, best_score);
}
```

## Custom Layout Algorithms

### Implementing Custom Layout

```rust
use reflow_network::graph::{CustomLayout, LayoutContext};

struct CircularLayout {
    radius: f64,
    start_angle: f64,
}

impl CustomLayout for CircularLayout {
    fn calculate_positions(&self, context: &LayoutContext) -> HashMap<String, Position> {
        let mut positions = HashMap::new();
        let node_count = context.nodes.len();
        let angle_step = 2.0 * std::f64::consts::PI / node_count as f64;
        
        for (i, node_id) in context.nodes.iter().enumerate() {
            let angle = self.start_angle + i as f64 * angle_step;
            let x = self.radius * angle.cos();
            let y = self.radius * angle.sin();
            
            positions.insert(node_id.clone(), Position { x, y, anchor: None });
        }
        
        positions
    }
}

// Use custom layout
let circular = CircularLayout {
    radius: 200.0,
    start_angle: 0.0,
};

let positions = graph.apply_custom_layout(&circular)?;
```

### Layout Plugins

```rust
// Register layout plugin
graph.register_layout_plugin("spiral", Box::new(SpiralLayout::new()));

// Use registered plugin
let config = LayoutConfig {
    algorithm: LayoutAlgorithm::Custom("spiral".to_string()),
    ..Default::default()
};

graph.calculate_layout_with_config(&config);
```

## Layout Events

### Listening to Layout Changes

```rust
use reflow_network::graph::LayoutEvents;

// Subscribe to layout events
let layout_receiver = graph.layout_event_channel.1.clone();

std::thread::spawn(move || {
    while let Ok(event) = layout_receiver.recv() {
        match event {
            LayoutEvents::LayoutStarted { algorithm } => {
                println!("Layout started: {:?}", algorithm);
            }
            LayoutEvents::LayoutCompleted { algorithm, duration } => {
                println!("Layout completed: {:?} in {:?}", algorithm, duration);
            }
            LayoutEvents::NodePositionChanged { node_id, old_pos, new_pos } => {
                println!("Node {} moved: ({:.1}, {:.1}) -> ({:.1}, {:.1})", 
                    node_id, old_pos.x, old_pos.y, new_pos.x, new_pos.y);
            }
            LayoutEvents::LayoutProgress { progress } => {
                println!("Layout progress: {:.1}%", progress * 100.0);
            }
        }
    }
});
```

## WebAssembly Layout API

### JavaScript Integration

```javascript
import { Graph, LayoutAlgorithm } from 'reflow-network';

const graph = new Graph("LayoutDemo", false, {});

// Add nodes and connections
graph.addNode("input", "InputNode", {});
graph.addNode("processor", "ProcessorNode", {});
graph.addNode("output", "OutputNode", {});
graph.addConnection("input", "out", "processor", "in", {});
graph.addConnection("processor", "out", "output", "in", {});

// Apply automatic layout
const positions = graph.calculateLayout({
    algorithm: LayoutAlgorithm.Hierarchical,
    nodeSpacing: 120,
    layerSpacing: 80
});

// Update UI with calculated positions
for (const [nodeId, position] of positions) {
    const nodeElement = document.getElementById(nodeId);
    nodeElement.style.left = `${position.x}px`;
    nodeElement.style.top = `${position.y}px`;
}

// Manual positioning
graph.setNodePosition("processor", 200, 100);

// Listen for layout events
graph.onLayoutChange((event) => {
    if (event.type === 'position_changed') {
        updateNodeElement(event.nodeId, event.newPosition);
    }
});
```

## Layout Best Practices

### Performance Optimization

1. **Use appropriate algorithms**: Choose the right algorithm for your graph type
2. **Limit iterations**: Set reasonable iteration limits for force-directed layouts
3. **Cache layouts**: Store calculated positions to avoid recalculation
4. **Incremental updates**: Use incremental layout for small changes

```rust
// Good: Incremental update for small changes
graph.add_node("new_node", "Component", None);
graph.incremental_layout_update(&incremental_config)?;

// Avoid: Full recalculation for small changes
graph.auto_layout()?; // Expensive for large graphs
```

### Visual Quality

1. **Minimize crossings**: Use algorithms that reduce edge crossings
2. **Consistent spacing**: Maintain uniform spacing between nodes
3. **Respect hierarchy**: Use hierarchical layout for workflow graphs
4. **Group related nodes**: Use group layouts for related components

```rust
// Good: Group-aware layout
let group_config = GroupLayoutConfig {
    group_spacing: 200.0,
    internal_spacing: 50.0,
    group_padding: 20.0,
    layout_algorithm: LayoutAlgorithm::Grid,
};
graph.layout_groups(&group_config)?;
```

### User Experience

1. **Smooth transitions**: Use animation between layout changes
2. **Preserve user positioning**: Respect manually positioned nodes
3. **Provide layout options**: Allow users to choose layout algorithms
4. **Show progress**: Display progress for long-running layout calculations

```rust
// Preserve manual positions
let manual_positions = graph.get_manually_positioned_nodes();
let config = LayoutConfig {
    preserve_positions: manual_positions,
    ..Default::default()
};
```

## Troubleshooting

### Common Layout Issues

1. **Overlapping nodes**: Increase node spacing or use different algorithm
2. **Poor aspect ratio**: Adjust layout bounds or use compact layout
3. **Too many crossings**: Use hierarchical layout or enable crossing minimization
4. **Unstable force layout**: Reduce spring strength or increase damping

```rust
// Fix overlapping nodes
let config = LayoutConfig {
    node_spacing: 150.0, // Increase spacing
    collision_detection: true,
    ..Default::default()
};

// Fix unstable force layout
let force_config = ForceDirectedConfig {
    spring_strength: 0.1, // Reduce from default 0.3
    damping: 0.8,         // Add damping
    ..Default::default()
};
```

## Next Steps

- [Advanced Features](advanced.md) - History, subgraphs, and optimization
- [Creating Graphs](creating-graphs.md) - Basic graph operations
- [Graph Analysis](analysis.md) - Validation and performance analysis
- [Building Visual Editors](../../tutorials/building-visual-editor.md) - Complete tutorial
