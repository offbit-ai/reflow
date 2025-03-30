export const Namespace = `flow`;

let graph_worker;

export function getGraphWorker() {
  if (!graph_worker) {
    graph_worker = new Worker("./worker.js", { type: "module" });
  }
  return graph_worker;
}

export function getTranslateXY(element) {
  const style = window.getComputedStyle(element)
  const matrix = new DOMMatrixReadOnly(style.transform)
  return {
    x: matrix.m41,
    y: matrix.m42
  }
}

/**
 * 
 * @param {Function} func 
 * @param {number} delay 
 * @returns 
 */
export function debounce(func, delay) {
  let timeoutId;
  return function (...args) {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => {
      func.apply(this, args);
    }, delay);
  };
}


