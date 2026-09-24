import { useState } from 'react';

type SkinImageProps = {
  src?: string | null;
  alt: string;
  accent?: string;
  className?: string;
  loading?: 'eager' | 'lazy';
  width?: number;
  height?: number;
};

// A deliberately neutral, local fallback. It signals unavailable artwork without
// inventing a skin or silently substituting a different catalog item.
const FALLBACK_ART = `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 640 400'%3E%3Crect width='640' height='400' fill='%23131921'/%3E%3Cpath d='M0 330 640 70M-40 390 600 130' stroke='%23303b49' stroke-width='2'/%3E%3Ctext x='320' y='215' fill='%23788798' font-family='Arial,sans-serif' font-size='20' text-anchor='middle' letter-spacing='3'%3EARTWORK UNAVAILABLE%3C/text%3E%3C/svg%3E`;

export function SkinImage({ src, alt, accent = '#58d6e7', className = '', loading = 'lazy', width = 640, height = 400 }: SkinImageProps) {
  const [failed, setFailed] = useState(!src);
  const [loaded, setLoaded] = useState(false);
  const imageSrc = failed ? FALLBACK_ART : src!;

  return (
    <div
      className={`skin-image image-${failed ? 'error' : loaded ? 'ready' : 'loading'} ${className}`.trim()}
      style={{ '--accent': accent, position: 'absolute', inset: 0, aspectRatio: `${width} / ${height}` } as React.CSSProperties}
      data-image-state={failed ? 'error' : loaded ? 'ready' : 'loading'}
    >
      <img
        src={imageSrc}
        alt={alt}
        width={width}
        height={height}
        loading={loading}
        style={{ opacity: loaded || failed ? 1 : 0 }}
        onLoad={() => setLoaded(true)}
        onError={() => { setFailed(true); setLoaded(true); }}
      />
      {!loaded && <span className="skin-image-loading" aria-hidden="true" />}
    </div>
  );
}

export { FALLBACK_ART };
