const rightClosePanel = document.createElement("div");
rightClosePanel.innerHTML = '<svg width="20" height="20" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M6.5 6.75v6.5L10 10 6.5 6.75ZM4.396 17c-.39 0-.72-.135-.99-.406-.27-.27-.406-.6-.406-.99V4.396c0-.39.135-.72.406-.99.27-.27.6-.406.99-.406h11.208c.39 0 .72.135.99.406.27.27.406.6.406.99v11.208c0 .39-.135.72-.406.99-.27.27-.6.406-.99.406H4.396ZM13 15.5h2.5v-11H13v11Zm-1.5 0v-11h-7v11h7Z" /></svg>';
const rightOpenPanel = document.createElement("div");
rightOpenPanel.innerHTML = '<svg width="20" height="20" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M9.5 13.25v-6.5L6 10l3.5 3.25ZM4.396 17c-.39 0-.72-.135-.99-.406-.27-.27-.406-.6-.406-.99V4.396c0-.39.135-.72.406-.99.27-.27.6-.406.99-.406h11.208c.39 0 .72.135.99.406.27.27.406.6.406.99v11.208c0 .39-.135.72-.406.99-.27.27-.6.406-.99.406H4.396ZM13 15.5h2.5v-11H13v11Zm-1.5 0v-11h-7v11h7Z"/></svg>';

const topClosePanel = document.createElement("div");
const topOpenPanel = document.createElement("div");
topOpenPanel.innerHTML = '<svg width="20" height="20" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="m10 10-3.25 3.5h6.5L10 10Zm-5.604 7c-.39 0-.72-.135-.99-.406-.27-.27-.406-.6-.406-.99V4.396c0-.39.135-.72.406-.99.27-.27.6-.406.99-.406h11.208c.39 0 .72.135.99.406.27.27.406.6.406.99v11.208c0 .39-.135.72-.406.99-.27.27-.6.406-.99.406H4.396ZM15.5 7V4.5h-11V7h11Zm-11 1.5v7h11v-7h-11Z" /></svg>';
topClosePanel.innerHTML = '<svg width="20" height="20" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="m10 14 3.25-3.5h-6.5L10 14Zm-5.604 3c-.39 0-.72-.135-.99-.406-.27-.27-.406-.6-.406-.99V4.396c0-.39.135-.72.406-.99.27-.27.6-.406.99-.406h11.208c.39 0 .72.135.99.406.27.27.406.6.406.99v11.208c0 .39-.135.72-.406.99-.27.27-.6.406-.99.406H4.396ZM15.5 7V4.5h-11V7h11Zm-11 1.5v7h11v-7h-11Z" /></svg>';

const nodesIcon = document.createElement("div")
nodesIcon.innerHTML = '<svg width="20" height="20" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="m10 17-7-5.5 1.208-.958L10 15.083l5.792-4.541L17 11.5 10 17Zm0-4L3 7.5 10 2l7 5.5-7 5.5Z" /></svg>';

const Icons = {
    rightClosePanel,
    rightOpenPanel,
    topClosePanel,
    topOpenPanel,
    nodesIcon
}

export default Icons;