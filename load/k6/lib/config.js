export function baseUrl() {
  const value = __ENV.BASE_URL || '';
  if (!/^https:\/\/[^/]+$/.test(value)) throw new Error('BASE_URL must be an HTTPS origin');
  return value;
}
export function options() { return { vus: Number(__ENV.VUS || 1), duration: __ENV.DURATION || '10s' }; }
