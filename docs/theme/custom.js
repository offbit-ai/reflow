// Custom JavaScript for Reflow Documentation

(function() {
    'use strict';

    // Wait for the page to load
    document.addEventListener('DOMContentLoaded', function() {
        initializeEnhancements();
    });

    function initializeEnhancements() {
        addCopyCodeButtons();
        enhanceTableOfContents();
        addScrollToTop();
        enhanceNavigation();
        addKeyboardShortcuts();
        initializeSearchEnhancements();
    }

    // Add copy buttons to code blocks
    function addCopyCodeButtons() {
        const codeBlocks = document.querySelectorAll('pre code');
        
        codeBlocks.forEach((block, index) => {
            const pre = block.parentElement;
            if (pre.querySelector('.copy-button')) return; // Already added
            
            const button = document.createElement('button');
            button.className = 'copy-button';
            button.innerHTML = '📋 Copy';
            button.setAttribute('aria-label', 'Copy code to clipboard');
            
            // Position the button
            pre.style.position = 'relative';
            button.style.position = 'absolute';
            button.style.top = '10px';
            button.style.right = '10px';
            button.style.background = 'var(--reflow-primary)';
            button.style.color = 'white';
            button.style.border = 'none';
            button.style.borderRadius = '4px';
            button.style.padding = '4px 8px';
            button.style.fontSize = '0.8em';
            button.style.cursor = 'pointer';
            button.style.transition = 'all 0.2s ease';
            button.style.zIndex = '10';
            
            button.addEventListener('click', async function() {
                try {
                    await navigator.clipboard.writeText(block.textContent);
                    button.innerHTML = '✅ Copied!';
                    button.style.background = 'var(--reflow-success)';
                    
                    setTimeout(() => {
                        button.innerHTML = '📋 Copy';
                        button.style.background = 'var(--reflow-primary)';
                    }, 2000);
                } catch (err) {
                    console.error('Failed to copy text: ', err);
                    button.innerHTML = '❌ Failed';
                    button.style.background = 'var(--reflow-error)';
                    
                    setTimeout(() => {
                        button.innerHTML = '📋 Copy';
                        button.style.background = 'var(--reflow-primary)';
                    }, 2000);
                }
            });
            
            pre.appendChild(button);
        });
    }

    // Enhance table of contents with expand/collapse
    function enhanceTableOfContents() {
        const chapters = document.querySelectorAll('.chapter');
        
        chapters.forEach(chapter => {
            const hasChildren = chapter.querySelector('ol');
            if (hasChildren) {
                const link = chapter.querySelector('a');
                if (link) {
                    const toggle = document.createElement('span');
                    toggle.className = 'chapter-toggle';
                    toggle.innerHTML = '▼';
                    toggle.style.cursor = 'pointer';
                    toggle.style.marginRight = '8px';
                    toggle.style.fontSize = '0.8em';
                    toggle.style.color = 'var(--reflow-primary)';
                    toggle.style.transition = 'transform 0.2s ease';
                    
                    link.insertBefore(toggle, link.firstChild);
                    
                    toggle.addEventListener('click', function(e) {
                        e.preventDefault();
                        e.stopPropagation();
                        
                        const ol = chapter.querySelector('ol');
                        if (ol) {
                            const isExpanded = ol.style.display !== 'none';
                            ol.style.display = isExpanded ? 'none' : 'block';
                            toggle.style.transform = isExpanded ? 'rotate(-90deg)' : 'rotate(0deg)';
                        }
                    });
                }
            }
        });
    }

    // Add scroll to top button
    function addScrollToTop() {
        const button = document.createElement('button');
        button.className = 'scroll-to-top';
        button.innerHTML = '↑';
        button.setAttribute('aria-label', 'Scroll to top');
        
        // Style the button
        Object.assign(button.style, {
            position: 'fixed',
            bottom: '20px',
            right: '20px',
            width: '50px',
            height: '50px',
            borderRadius: '50%',
            background: 'var(--reflow-primary)',
            color: 'white',
            border: 'none',
            fontSize: '18px',
            cursor: 'pointer',
            opacity: '0',
            transition: 'all 0.3s ease',
            zIndex: '1000',
            boxShadow: '0 4px 8px rgba(0, 0, 0, 0.2)'
        });
        
        document.body.appendChild(button);
        
        // Show/hide button based on scroll position
        window.addEventListener('scroll', function() {
            if (window.pageYOffset > 300) {
                button.style.opacity = '1';
                button.style.transform = 'translateY(0)';
            } else {
                button.style.opacity = '0';
                button.style.transform = 'translateY(10px)';
            }
        });
        
        // Scroll to top functionality
        button.addEventListener('click', function() {
            window.scrollTo({
                top: 0,
                behavior: 'smooth'
            });
        });
    }

    // Enhance navigation with progress indicator
    function enhanceNavigation() {
        const progressBar = document.createElement('div');
        progressBar.className = 'reading-progress';
        
        Object.assign(progressBar.style, {
            position: 'fixed',
            top: '0',
            left: '0',
            width: '0%',
            height: '3px',
            background: 'linear-gradient(90deg, var(--reflow-primary), var(--reflow-accent))',
            zIndex: '1001',
            transition: 'width 0.1s ease'
        });
        
        document.body.appendChild(progressBar);
        
        // Update progress bar on scroll
        window.addEventListener('scroll', function() {
            const scrollTop = window.pageYOffset;
            const docHeight = document.documentElement.scrollHeight - window.innerHeight;
            const scrollPercent = (scrollTop / docHeight) * 100;
            
            progressBar.style.width = Math.min(scrollPercent, 100) + '%';
        });
    }

    // Add keyboard shortcuts
    function addKeyboardShortcuts() {
        document.addEventListener('keydown', function(e) {
            // Only trigger shortcuts when not typing in input fields
            if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
                return;
            }
            
            switch(e.key) {
                case 's':
                case 'S':
                    if (e.ctrlKey || e.metaKey) {
                        e.preventDefault();
                        const searchBar = document.getElementById('searchbar');
                        if (searchBar) {
                            searchBar.focus();
                        }
                    }
                    break;
                    
                case 'h':
                case 'H':
                    // Toggle sidebar
                    const sidebarToggle = document.getElementById('sidebar-toggle');
                    if (sidebarToggle) {
                        sidebarToggle.click();
                    }
                    break;
                    
                case 't':
                case 'T':
                    // Toggle theme
                    const themeToggle = document.getElementById('theme-toggle');
                    if (themeToggle) {
                        themeToggle.click();
                    }
                    break;
                    
                case 'ArrowLeft':
                    // Previous page
                    const prevLink = document.querySelector('.nav-chapters .nav-previous');
                    if (prevLink) {
                        window.location.href = prevLink.href;
                    }
                    break;
                    
                case 'ArrowRight':
                    // Next page
                    const nextLink = document.querySelector('.nav-chapters .nav-next');
                    if (nextLink) {
                        window.location.href = nextLink.href;
                    }
                    break;
            }
        });
        
        // Add keyboard shortcut hints
        addKeyboardHints();
    }

    // Add keyboard shortcut hints
    function addKeyboardHints() {
        const hintsContainer = document.createElement('div');
        hintsContainer.className = 'keyboard-hints';
        hintsContainer.innerHTML = `
            <div class="hints-toggle" title="Keyboard Shortcuts">⌨️</div>
            <div class="hints-panel" style="display: none;">
                <h4>Keyboard Shortcuts</h4>
                <ul>
                    <li><kbd>Ctrl+S</kbd> - Focus search</li>
                    <li><kbd>H</kbd> - Toggle sidebar</li>
                    <li><kbd>T</kbd> - Toggle theme</li>
                    <li><kbd>←</kbd> - Previous page</li>
                    <li><kbd>→</kbd> - Next page</li>
                </ul>
            </div>
        `;
        
        // Style the hints
        Object.assign(hintsContainer.style, {
            position: 'fixed',
            top: '80px',
            right: '20px',
            zIndex: '999'
        });
        
        const hintsPanel = hintsContainer.querySelector('.hints-panel');
        Object.assign(hintsPanel.style, {
            background: 'var(--theme-popup-bg)',
            border: '1px solid var(--theme-popup-border)',
            borderRadius: '8px',
            padding: '16px',
            boxShadow: '0 4px 12px rgba(0, 0, 0, 0.15)',
            minWidth: '200px'
        });
        
        const toggle = hintsContainer.querySelector('.hints-toggle');
        Object.assign(toggle.style, {
            background: 'var(--reflow-primary)',
            color: 'white',
            width: '40px',
            height: '40px',
            borderRadius: '50%',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            cursor: 'pointer',
            fontSize: '16px'
        });
        
        toggle.addEventListener('click', function() {
            const panel = hintsContainer.querySelector('.hints-panel');
            panel.style.display = panel.style.display === 'none' ? 'block' : 'none';
        });
        
        document.body.appendChild(hintsContainer);
    }

    // Enhance search functionality
    function initializeSearchEnhancements() {
        const searchBar = document.getElementById('searchbar');
        if (!searchBar) return;
        
        // Add search suggestions (basic implementation)
        const suggestionsContainer = document.createElement('div');
        suggestionsContainer.className = 'search-suggestions';
        suggestionsContainer.style.display = 'none';
        
        searchBar.parentNode.insertBefore(suggestionsContainer, searchBar.nextSibling);
        
        // Debounced search
        let searchTimeout;
        searchBar.addEventListener('input', function() {
            clearTimeout(searchTimeout);
            searchTimeout = setTimeout(() => {
                // Custom search logic can be added here
            }, 300);
        });
    }

    // Add utility functions for external use
    window.ReflowDocs = {
        scrollToElement: function(selector) {
            const element = document.querySelector(selector);
            if (element) {
                element.scrollIntoView({ behavior: 'smooth' });
            }
        },
        
        highlightCode: function(language) {
            // Placeholder for code highlighting functionality
            console.log('Highlighting code for language:', language);
        },
        
        toggleSection: function(sectionId) {
            const section = document.getElementById(sectionId);
            if (section) {
                section.style.display = section.style.display === 'none' ? 'block' : 'none';
            }
        }
    };

    // Console welcome message
    console.log(`
    🔥 Reflow Documentation
    
    Welcome to the Reflow documentation! 
    
    Keyboard shortcuts:
    • Ctrl+S: Focus search
    • H: Toggle sidebar  
    • T: Toggle theme
    • ←/→: Navigate pages
    
    For more help, visit: https://reflow.dev
    `);

})();
