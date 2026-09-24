import { useLayoutEffect, useRef, useState } from 'react';
import type { SkinDefinition } from '../types';

type SkinImageProps = {
  src?: string | null;
  alt: string;
  accent?: string;
  className?: string;
  loading?: 'eager' | 'lazy';
  width?: number;
  height?: number;
  layout?: 'fill' | 'inline';
};

// A deliberately neutral, local fallback. It signals unavailable artwork without
// inventing a skin or silently substituting a different catalog item.
const FALLBACK_ART = `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 640 400'%3E%3Crect width='640' height='400' fill='%23131921'/%3E%3Cpath d='M0 330 640 70M-40 390 600 130' stroke='%23303b49' stroke-width='2'/%3E%3Ctext x='320' y='215' fill='%23788798' font-family='Arial,sans-serif' font-size='20' text-anchor='middle' letter-spacing='3'%3EARTWORK UNAVAILABLE%3C/text%3E%3C/svg%3E`;

/** Prefer the API/catalog-provided canonical asset and never invent artwork. */
export function resolveSkinImage(item: Pick<SkinDefinition, 'image' | 'canonicalImageUrl'>): string | null {
  return item.canonicalImageUrl || item.image || null;
}

export function SkinImage({ src, alt, accent = '#58d6e7', className = '', loading = 'lazy', width = 640, height = 400, layout = 'fill' }: SkinImageProps) {
  const source = src?.trim() || null;
  const imageRef = useRef<HTMLImageElement>(null);
  const [state, setState] = useState<{ source: string | null; failed: boolean; loaded: boolean }>({ source, failed: !source, loaded: !source });
  const isCurrentSource = state.source === source;
  const failed = isCurrentSource ? state.failed : !source;
  const loaded = isCurrentSource ? state.loaded : !source;
  const imageSrc = failed ? FALLBACK_ART : source!;

  useLayoutEffect(() => {
    const image = imageRef.current;
    if (!source) {
      setState({ source, failed: true, loaded: true });
    } else if (image?.complete) {
      setState({ source, failed: image.naturalWidth === 0, loaded: true });
    } else {
      setState({ source, failed: false, loaded: false });
    }
  }, [source]);

  return (
    <div
      className={`skin-image image-${failed ? 'error' : loaded ? 'ready' : 'loading'} ${className}`.trim()}
      style={{ '--accent': accent, position: layout === 'fill' ? 'absolute' : 'relative', inset: layout === 'fill' ? 0 : 'auto', aspectRatio: `${width} / ${height}` } as React.CSSProperties}
      data-image-state={failed ? 'error' : loaded ? 'ready' : 'loading'}
    >
      <img
        ref={imageRef}
        key={imageSrc}
        src={imageSrc}
        alt={alt}
        width={width}
        height={height}
        loading={loading}
        style={{ opacity: loaded || failed ? 1 : 0 }}
        onLoad={(event) => {
          if (source && event.currentTarget.getAttribute('src') === source) setState({ source, failed: false, loaded: true });
        }}
        onError={(event) => {
          if (source && event.currentTarget.getAttribute('src') === source) setState({ source, failed: true, loaded: true });
        }}
      />
      {!loaded && !failed && <span className="skin-image-loading" aria-hidden="true" />}
    </div>
  );
}

export { FALLBACK_ART };
