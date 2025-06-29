//! Basic example showing Node.js reflow_network usage
//! 
//! This example demonstrates that our Node.js bindings mirror the WASM API
//! but with enhanced capabilities like full async support and native threading

const reflow = require('../index.node');

async function main() {
    console.log('🚀 ReFlow Network Node.js Example');
    console.log('=====================================');
    
    // Initialize panic hook for better error reporting
    reflow.init_panic_hook();
    
    // Create Network instances - Enhanced versions with full Node.js capabilities
    console.log('\n📡 Creating Network instances...');
    const Network = reflow.Network;
    const GraphNetwork = reflow.GraphNetwork;
    
    const network = new Network();
    const graphNetwork = new GraphNetwork();
    
    console.log('✅ Network instances created successfully');
    
    // Create Graph instances - Clean names without WASM* prefix
    console.log('\n📊 Creating Graph instances...');
    const Graph = reflow.Graph;
    const GraphHistory = reflow.GraphHistory;
    
    const graph = new Graph();
    const history = new GraphHistory();
    
    console.log('✅ Graph instances created successfully');
    
    // Create Actor instances
    console.log('\n🎭 Creating Actor instances...');
    const Actor = reflow.Actor;
    const actor = new Actor();
    
    console.log('✅ Actor instances created successfully');
    
    // Create Multi-graph instances - Workspace and composition features
    console.log('\n🔗 Creating Multi-graph instances...');
    const Workspace = reflow.Workspace;
    const MultiGraphNetwork = reflow.MultiGraphNetwork;
    const NamespaceManager = reflow.NamespaceManager;
    const GraphDependencyManager = reflow.GraphDependencyManager;
    
    const workspace = new Workspace();
    const multiGraphNetwork = new MultiGraphNetwork();
    const namespaceManager = new NamespaceManager();
    const dependencyManager = new GraphDependencyManager();
    
    console.log('✅ Multi-graph instances created successfully');
    
    // Create Error types - Clean names, no WASM* prefix
    console.log('\n⚠️ Creating Error types...');
    const CompositionError = reflow.CompositionError;
    const ValidationError = reflow.ValidationError;
    const LoadError = reflow.LoadError;
    const NamespaceError = reflow.NamespaceError;
    
    const compositionError = new CompositionError('Test composition error');
    const validationError = new ValidationError('Test validation error');
    const loadError = new LoadError('Test load error');
    const namespaceError = new NamespaceError('Test namespace error');
    
    console.log('✅ Error instances created successfully');
    
    console.log('\n🎉 All ReFlow Network components initialized successfully!');
    console.log('\n💡 Key advantages of Node.js version over WASM:');
    console.log('   • Full native async/await support via Tokio');
    console.log('   • Access to filesystem operations');
    console.log('   • Full networking capabilities');
    console.log('   • Multi-threaded actor execution');
    console.log('   • Better error handling and debugging');
    console.log('   • Native performance without WASM overhead');
    
    console.log('\n✨ API Surface mirrors WASM version but with enhanced capabilities!');
}

// Handle errors gracefully
main().catch(error => {
    console.error('❌ Error running example:', error);
    process.exit(1);
});
