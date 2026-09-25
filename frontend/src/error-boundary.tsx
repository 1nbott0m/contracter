import { Component, type ErrorInfo, type ReactNode } from 'react';

type Props = { children: ReactNode };
type State = { hasError: boolean };

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('CONTRACTER frontend render failed', error, info);
  }

  private retry = () => this.setState({ hasError: false });

  render() {
    if (this.state.hasError) {
      return <main className="fatal-error" role="alert">
        <span className="eyebrow">CONTRACTER / RECOVERY</span>
        <h1>Интерфейс не загрузился</h1>
        <p>Произошла непредвиденная ошибка отображения. Данные контракта не были подменены.</p>
        <button className="primary route-state-action" type="button" onClick={this.retry}>Повторить</button>
      </main>;
    }
    return this.props.children;
  }
}
