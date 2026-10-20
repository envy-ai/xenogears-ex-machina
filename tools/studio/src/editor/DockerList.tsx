// Copied from rpg_creator src/editor/DockerList.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import type {
  ButtonHTMLAttributes,
  CSSProperties,
  HTMLAttributes,
  ReactNode
} from 'react';
import {
  ReorderGrip,
  type ReorderHandleProps,
  type ReorderTargetProps
} from './itemReorder';

function classNames(
  ...values: Array<string | false | null | undefined>
): string {
  return values.filter(Boolean).join(' ');
}

export function DockerList({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={classNames('docker-list-layout', className)}
      {...props}
    />
  );
}

export function DockerListStack({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={classNames('docker-list-stack', className)}
      {...props}
    />
  );
}

export function DockerListHeader({
  className,
  stacked = false,
  ...props
}: HTMLAttributes<HTMLDivElement> & { stacked?: boolean }) {
  return (
    <div
      className={classNames(
        'docker-list-header',
        stacked && 'docker-list-header--stacked',
        className
      )}
      {...props}
    />
  );
}

export function DockerListBody({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={classNames('docker-item-list', className)}
      {...props}
    />
  );
}

type DockerListRowStyle = CSSProperties & {
  '--docker-list-row-columns'?: string;
};

export function DockerListRow({
  children,
  className,
  selected = false,
  intrinsicHeight = false,
  columns,
  reorder,
  style,
  ...props
}: HTMLAttributes<HTMLDivElement> & {
  selected?: boolean;
  intrinsicHeight?: boolean;
  columns?: string;
  reorder?: {
    label: string;
    handleProps: ReorderHandleProps;
    targetProps: ReorderTargetProps;
    enabled?: boolean;
    surface?: 'grip' | 'row';
  };
}) {
  const rowStyle: DockerListRowStyle = {
    ...style,
    '--docker-list-row-columns': columns
  };
  return (
    <div
      className={classNames(
        'editor-list-row',
        intrinsicHeight && 'editor-list-row--intrinsic-height',
        'docker-item',
        selected && 'active',
        className
      )}
      data-selected={selected ? 'true' : undefined}
      data-reorder-surface={reorder?.surface}
      style={rowStyle}
      {...reorder?.targetProps}
      {...(reorder?.surface === 'row' ? reorder.handleProps : undefined)}
      {...props}
    >
      {children}
      {reorder && reorder.surface !== 'row' ? (
        <ReorderGrip
          label={reorder.label}
          props={reorder.handleProps}
          enabled={reorder.enabled}
        />
      ) : null}
    </div>
  );
}

export function DockerListSelect({
  className,
  selected = false,
  type = 'button',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  selected?: boolean;
}) {
  return (
    <button
      type={type}
      className={classNames(
        'editor-button',
        'editor-button--unstyled',
        'docker-item-select',
        selected && 'active',
        className
      )}
      data-selected={selected ? 'true' : undefined}
      aria-current={selected ? 'true' : undefined}
      {...props}
    />
  );
}

export function EditorListTitle({
  children,
  className,
  title,
  ...props
}: HTMLAttributes<HTMLSpanElement> & {
  title: string;
  children?: ReactNode;
}) {
  return (
    <span
      className={classNames('editor-list-title', className)}
      title={title}
      {...props}
    >
      {children ?? title}
    </span>
  );
}

export function DockerListFooter({
  className,
  summary = false,
  ...props
}: HTMLAttributes<HTMLDivElement> & { summary?: boolean }) {
  return (
    <div
      className={classNames(
        'docker-list-footer',
        summary && 'docker-list-footer--summary',
        className
      )}
      {...props}
    />
  );
}

export function DockerListEmpty({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement> & { children: ReactNode }) {
  return (
    <div
      className={classNames(
        'editor-text',
        'editor-text--empty',
        'docker-list-empty',
        className
      )}
      role="status"
      {...props}
    >
      {children}
    </div>
  );
}
