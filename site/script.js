const copyButton = document.querySelector('.copy');
const command = document.querySelector('#install-command');
const status = document.querySelector('#copy-status');
let resetTimer;

copyButton.addEventListener('click', async () => {
  clearTimeout(resetTimer);
  try {
    await navigator.clipboard.writeText(command.textContent);
    copyButton.dataset.copied = '';
    copyButton.setAttribute('aria-label', 'Install command copied');
    status.textContent = 'Copied to clipboard.';
    resetTimer = setTimeout(() => {
      delete copyButton.dataset.copied;
      copyButton.setAttribute('aria-label', 'Copy install command');
      status.textContent = '';
    }, 2500);
  } catch {
    delete copyButton.dataset.copied;
    copyButton.setAttribute('aria-label', 'Copy install command');
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(command);
    selection.removeAllRanges();
    selection.addRange(range);
    status.textContent = 'Select and copy the command with Ctrl+C or ⌘C.';
  }
});
