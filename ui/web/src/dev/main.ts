// Development entry (`dev.html`): the real UI on the fake service, in a browser.
import { start, startBench } from '../start';
import { FakeService } from './fake-service';

if (location.hash === '#bench-canvas') {
  void startBench().then((r) => {
    console.log(JSON.stringify(r, null, 2));
    document.body.insertAdjacentHTML(
      'beforeend',
      `<pre id="bench-report">${JSON.stringify(r, null, 2)}</pre>`,
    );
  });
} else {
  start(new FakeService());
}
