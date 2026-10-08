// Development entry (`dev.html`): the real UI on the fake service, in a browser.
import { start } from '../start';
import { FakeService } from './fake-service';

start(new FakeService());
