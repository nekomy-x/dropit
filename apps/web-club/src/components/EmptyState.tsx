import Icon from './Icon';

interface EmptyStateProps {
  title: string;
  detail: string;
  icon?: `lucide:${string}`;
}

export default function EmptyState({ title, detail, icon = 'lucide:circle-dashed' }: EmptyStateProps) {
  return (
    <div className="operation-empty" role="status">
      <span className="operation-empty-icon"><Icon name={icon} size={18} /></span>
      <div>
        <p className="operation-empty-title">{title}</p>
        <p className="operation-empty-detail">{detail}</p>
      </div>
    </div>
  );
}
