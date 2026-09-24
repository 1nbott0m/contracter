import http from 'k6/http';
import { check } from 'k6';
import { baseUrl, options } from './lib/config.js';
export const options = options();
export default function () { const response = http.get(`${baseUrl()}/health/live`); check(response, { 'live': r => r.status === 200 }); }
