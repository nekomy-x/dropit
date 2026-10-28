import { useEffect, useRef, type ReactNode } from 'react';
import Icon from './Icon';
import { useLocale } from '../locale';

/** Native modal semantics provide focus containment and restore the invoking control. */
export default function AdminDialog({ title, children, onClose, wide = false }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const { t } = useLocale();
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    const previous = document.activeElement;
    dialog?.showModal();
    return () => { dialog?.close(); if (previous instanceof HTMLElement) previous.focus(); };
  }, []);
  return <dialog ref={ref} aria-labelledby="admin-dialog-title" className={`admin-dialog ${wide ? 'admin-dialog-wide' : ''}`} onCancel={onClose} onClick={(event) => { if (event.target === event.currentTarget) { const bounds = event.currentTarget.getBoundingClientRect(); if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) onClose(); } }}>
    <header><h2 id="admin-dialog-title">{title}</h2><button type="button" className="admin-icon-button" aria-label={t('shared.closeDialog')} onClick={onClose}><Icon name="lucide:x" /></button></header>
    {children}
  </dialog>;
}
