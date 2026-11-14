const copyTimers = new WeakMap();

for (const button of document.querySelectorAll('[data-copy-target]')) {
  const command = document.getElementById(button.dataset.copyTarget);
  if (!command) continue;
  const label = button.getAttribute('aria-label');
  const status = document.querySelector('#copy-status');
  const text = button.textContent;

  button.addEventListener('click', async () => {
    clearTimeout(copyTimers.get(button));
    try {
      await navigator.clipboard.writeText(command.textContent);
      button.dataset.copied = '';
      button.setAttribute('aria-label', 'Copied to clipboard');
      if (button.classList.contains('code-copy')) button.textContent = 'Copied';
      if (status) status.textContent = 'Copied to clipboard.';
      copyTimers.set(button, setTimeout(() => {
        delete button.dataset.copied;
        button.setAttribute('aria-label', label);
        if (button.classList.contains('code-copy')) button.textContent = text;
        if (status) status.textContent = '';
      }, 2500));
    } catch {
      delete button.dataset.copied;
      button.setAttribute('aria-label', label);
      if (button.classList.contains('code-copy')) button.textContent = text;
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(command);
      selection.removeAllRanges();
      selection.addRange(range);
      if (status) status.textContent = 'Select and copy the code with Ctrl+C or ⌘C.';
    }
  });
}
