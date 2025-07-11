// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded affix "><a href="index.html">Introduction</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Getting Started</li><li class="chapter-item expanded "><a href="getting-started/index.html"><strong aria-hidden="true">1.</strong> Getting Started</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="getting-started/installation.html"><strong aria-hidden="true">1.1.</strong> Installation</a></li><li class="chapter-item "><a href="getting-started/basic-concepts.html"><strong aria-hidden="true">1.2.</strong> Basic Concepts</a></li><li class="chapter-item "><a href="getting-started/development-setup.html"><strong aria-hidden="true">1.3.</strong> Development Setup</a></li><li class="chapter-item "><a href="getting-started/first-workflow.html"><strong aria-hidden="true">1.4.</strong> Your First Workflow</a></li></ol></li><li class="chapter-item expanded "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Architecture</li><li class="chapter-item expanded "><a href="architecture/overview.html"><strong aria-hidden="true">2.</strong> Architecture Overview</a></li><li class="chapter-item expanded "><a href="architecture/actor-model.html"><strong aria-hidden="true">3.</strong> Actor Model</a></li><li class="chapter-item expanded "><a href="architecture/message-passing.html"><strong aria-hidden="true">4.</strong> Message Passing</a></li><li class="chapter-item expanded "><a href="architecture/graph-system.html"><strong aria-hidden="true">5.</strong> Graph System</a></li><li class="chapter-item expanded "><a href="architecture/distributed-networks.html"><strong aria-hidden="true">6.</strong> Distributed Networks</a></li><li class="chapter-item expanded "><a href="architecture/multi-graph-composition.html"><strong aria-hidden="true">7.</strong> Multi-Graph Composition</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Observability</li><li class="chapter-item expanded "><a href="observability/overview.html"><strong aria-hidden="true">8.</strong> Observability Overview</a></li><li class="chapter-item expanded "><a href="observability/getting-started.html"><strong aria-hidden="true">9.</strong> Quick Start Guide</a></li><li class="chapter-item expanded "><a href="observability/architecture.html"><strong aria-hidden="true">10.</strong> Architecture</a></li><li class="chapter-item expanded "><a href="observability/event-types.html"><strong aria-hidden="true">11.</strong> Event Types</a></li><li class="chapter-item expanded "><a href="observability/data-flow-tracing.html"><strong aria-hidden="true">12.</strong> Data Flow Tracing</a></li><li class="chapter-item expanded "><a href="observability/configuration.html"><strong aria-hidden="true">13.</strong> Configuration</a></li><li class="chapter-item expanded "><a href="observability/storage-backends.html"><strong aria-hidden="true">14.</strong> Storage Backends</a></li><li class="chapter-item expanded "><a href="observability/deployment.html"><strong aria-hidden="true">15.</strong> Production Deployment</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">API Documentation</li><li class="chapter-item expanded "><a href="api/actors/creating-actors.html"><strong aria-hidden="true">16.</strong> Working with Actors</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="api/actors/actor-config.html"><strong aria-hidden="true">16.1.</strong> ActorConfig System</a></li></ol></li><li class="chapter-item expanded "><a href="api/graph/creating-graphs.html"><strong aria-hidden="true">17.</strong> Graph API</a><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="api/graph/analysis.html"><strong aria-hidden="true">17.1.</strong> Graph Analysis</a></li><li class="chapter-item "><a href="api/graph/layout.html"><strong aria-hidden="true">17.2.</strong> Graph Layout</a></li><li class="chapter-item "><a href="api/graph/advanced.html"><strong aria-hidden="true">17.3.</strong> Advanced Features</a></li></ol></li><li class="chapter-item expanded "><div><strong aria-hidden="true">18.</strong> Multi-Graph API</div><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="api/multi-graph/workspace-discovery.html"><strong aria-hidden="true">18.1.</strong> Workspace Discovery</a></li><li class="chapter-item "><a href="api/multi-graph/graph-composition.html"><strong aria-hidden="true">18.2.</strong> Graph Composition</a></li><li class="chapter-item "><a href="api/multi-graph/dependency-resolution.html"><strong aria-hidden="true">18.3.</strong> Dependency Resolution</a></li></ol></li><li class="chapter-item expanded "><div><strong aria-hidden="true">19.</strong> Distributed API</div><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="api/distributed/setting-up-networks.html"><strong aria-hidden="true">19.1.</strong> Setting Up Networks</a></li><li class="chapter-item "><a href="api/distributed/remote-actors.html"><strong aria-hidden="true">19.2.</strong> Remote Actors</a></li><li class="chapter-item "><a href="api/distributed/discovery-registration.html"><strong aria-hidden="true">19.3.</strong> Discovery &amp; Registration</a></li><li class="chapter-item "><a href="api/distributed/conflict-resolution.html"><strong aria-hidden="true">19.4.</strong> Conflict Resolution</a></li></ol></li><li class="chapter-item expanded "><div><strong aria-hidden="true">20.</strong> WASM API</div><a class="toggle"><div>❱</div></a></li><li><ol class="section"><li class="chapter-item "><a href="api/wasm/getting-started.html"><strong aria-hidden="true">20.1.</strong> Getting Started</a></li><li class="chapter-item "><a href="api/wasm/actors-in-browser.html"><strong aria-hidden="true">20.2.</strong> Actors in Browser</a></li></ol></li><li class="chapter-item expanded "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Components &amp; Scripting</li><li class="chapter-item expanded "><a href="components/standard-library.html"><strong aria-hidden="true">21.</strong> Standard Component Library</a></li><li class="chapter-item expanded "><a href="scripting/javascript/deno-runtime.html"><strong aria-hidden="true">22.</strong> JavaScript &amp; Deno Runtime</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Tutorials</li><li class="chapter-item expanded "><a href="tutorials/building-visual-editor.html"><strong aria-hidden="true">23.</strong> Building a Visual Editor</a></li><li class="chapter-item expanded "><a href="tutorials/reactflow-reflow-integration.html"><strong aria-hidden="true">24.</strong> ReactFlow Integration</a></li><li class="chapter-item expanded "><a href="tutorials/performance-optimization.html"><strong aria-hidden="true">25.</strong> Performance Optimization</a></li><li class="chapter-item expanded "><a href="tutorials/distributed-workflow-example.html"><strong aria-hidden="true">26.</strong> Distributed Workflow Example</a></li><li class="chapter-item expanded "><a href="tutorials/multi-graph-workspace.html"><strong aria-hidden="true">27.</strong> Multi-Graph Workspace</a></li><li class="chapter-item expanded "><a href="tutorials/wasm-actor-development.html"><strong aria-hidden="true">28.</strong> WASM Actor Development</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Migration Guides</li><li class="chapter-item expanded "><a href="migration/actor-config-migration.html"><strong aria-hidden="true">29.</strong> ActorConfig Migration</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Deployment</li><li class="chapter-item expanded "><a href="deployment/native-deployment.html"><strong aria-hidden="true">30.</strong> Native Deployment</a></li><li class="chapter-item expanded "><a href="deployment/browser-deployment.html"><strong aria-hidden="true">31.</strong> Browser Deployment</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Examples</li><li class="chapter-item expanded "><a href="examples/index.html"><strong aria-hidden="true">32.</strong> Example Projects</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Reference</li><li class="chapter-item expanded "><a href="reference/api-reference.html"><strong aria-hidden="true">33.</strong> API Reference</a></li><li class="chapter-item expanded "><a href="reference/troubleshooting-guide.html"><strong aria-hidden="true">34.</strong> Troubleshooting Guide</a></li><li class="chapter-item expanded affix "><li class="spacer"></li><li class="chapter-item expanded affix "><li class="part-title">Appendices</li><li class="chapter-item expanded "><a href="appendices/glossary.html"><strong aria-hidden="true">35.</strong> Glossary</a></li><li class="chapter-item expanded "><a href="appendices/contributing.html"><strong aria-hidden="true">36.</strong> Contributing</a></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split("#")[0].split("?")[0];
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
